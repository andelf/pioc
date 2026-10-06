# Embassy 风格 `pioc-programs` 高层协议 API：对比、设计与可行性验证

> 状态：设计已形成可编译原型。当前原型实现 `CommandPort`、WS2812/RGB
> 和官方 RGB1W 固件提供的 DS18x20 操作，并用官方 `RGB1W.BIN` 完成一次
> WS2812 单灯端到端 VM 验证。UART、I²C、通用 OneWire 和真实 CH32X035
> 硬件 backend 属于后续阶段。

## 1. 结论

Embassy RP PIO 最值得借鉴的不是 RP2040 的 FIFO、side-set 或多状态机本身，
而是以下软件结构：

1. 把“程序映像”和“使用该程序的协议驱动”分离；
2. 驱动独占状态机、pin、DMA 等资源，Rust 所有权防止并发误用；
3. 同时提供原始低层入口和 `write_slice`、`transfer`、`reset` 等协议 API；
4. 阻塞、异步 FIFO、DMA 和 IRQ 共享同一份协议语义；
5. 程序 ABI、pin、时钟、缓冲区布局由驱动封装，而不是暴露给应用。

WCH PIOC 可以采用同样分层，但不能复制 RP PIO 的具体运行模型。PIOC 是一个
8-bit 可编程核心，使用 `cmd.in/cmd.out`、32-byte 共享 SFR 和共享程序 SRAM；
它没有四个独立状态机和通用 TX/RX FIFO。因此建议把主抽象定义为
`CommandPort`/`EventPort`，而不是伪造 RP 风格的 FIFO。

当前验证说明以下设计可行：

- 协议 crate 可以保持 `no_std`、无堆分配和 executor 无关；
- 同一个高层 `Ws2812` 驱动可以由硬件 backend 或 VM backend 承载；
- 短 SFR 与长 RAM 两条固件路径可以由 API 自动选择；
- 固件命令冲突、容量和状态码可以在类型化错误中显式报告；
- 异步边界放在“等待 PIOC IRQ”处，符合官方中断式 UART/I²C 示例。

## 2. 调研依据

### 2.1 Embassy RP PIO

| 主题 | 参考实现 |
|---|---|
| 内联汇编与多状态机异步任务 | `examples/rp/src/bin/pio_async.rs` |
| TX/RX DMA | `examples/rp/src/bin/pio_dma.rs` |
| 阻塞与异步 SPI | `pio_spi.rs`、`pio_spi_async.rs` |
| WS2812 程序对象、DMA 与颜色顺序 | `embassy-rp/src/pio_programs/ws2812.rs` |
| OneWire reset/read/write/search/强上拉 | `embassy-rp/src/pio_programs/onewire.rs` |
| UART TX/RX 分离及 `embedded-io-async` | `embassy-rp/src/pio_programs/uart.rs` |
| IRQ future/flag | `tests/rp/src/bin/pio_irq.rs` |
| 程序自动重定位和空间管理 | `tests/rp/src/bin/pio_multi_load.rs` |

Embassy 中可见三种程序组织方式：

- `pio_asm!` 内联汇编；
- `pio::Assembler` 类型化 builder；
- `PioXxxProgram + PioXxx` 可复用协议驱动。

当前 Embassy tree 没有发现使用外部 `.pio` 文件或 `pio_file!` 的 RP 示例。

### 2.2 WCH PIOC

主要依据：

- `PIOC-EN.pdf`、`CHRISC8B-EN.pdf`；
- 官方 `RGB1W.ASM/BIN`；
- `RGB1W.c/.h` 主机代码；
- 官方 UART 与 I²C 主机中断/双缓冲代码；
- 当前仓库的 assembler、VM、SFR 文档和端到端 trace。

官方 RGB1W 主机代码已经展示两种数据路径：

| 模式 | 容量 | 装载条件 | 启动方式 |
|---|---:|---|---|
| SFR short | 1..32 byte | PIOC 运行时写 `d0..d31` | `cmd.in = length` 或 `length|0x40` |
| program-RAM long | 1..3072 byte | PIOC reset/停钟时写 SRAM `0x400..0xFFF` | `cmd.in = 0x80|bit_cycles` |

官方 UART 和 I²C 示例进一步证明：

- IRQ 不只表示最终 completion，也可以表示“请主机补下一块 buffer”；
- UART 使用两个 8-byte SFR buffer 交替 refill；
- I²C 使用多组 8-byte buffer 处理 controller/target 方向；
- backend 必须支持重复等待 IRQ，而不能把每个 IRQ 都硬编码为最终完成。

## 3. RP PIO 与 WCH PIOC 的关键差异

| 能力 | RP PIO / Embassy | WCH PIOC | API 影响 |
|---|---|---|---|
| 执行单元 | 每个 block 多个 state machine | 单个 RISC8B/PIOC core | 驱动默认独占整个 PIOC |
| 程序空间 | 32 word，共享、可重定位 | 2048 word 完整映像 | 不需要通用片段重定位 |
| 数据通道 | TX/RX FIFO，可 DMA pacing | `cmd.*`、`xchg`、`d0..d31`、共享 SRAM | 使用命令/事件 backend |
| pin 映射 | 每个 SM 可配置 IN/OUT/SET/side-set | IO0/IO1 及芯片 pin remap | pin 是固件 ABI 的一部分 |
| 时钟 | 每个 SM 独立 divider | PIOC 随系统时钟，固件延时依赖频率 | Program profile 必须记录时钟 |
| IRQ | 多个 PIO IRQ flag | PIOC 请求主机 IRQ | backend 统一为异步事件 |
| 并发 | 多 SM 可同时工作 | 一个 core 一次运行一个固件 | 不提供虚假的多驱动并发 |
| 数据流 | FIFO stream 自然 | 固件自定义双缓冲/握手 | streaming 必须按固件 ABI 建模 |

## 4. 目标与非目标

### 4.1 目标

- `no_std`，默认不依赖 allocator；
- executor 无关，可适配 Embassy、轮询 HAL 和 VM；
- 硬件/VM 复用同一协议驱动；
- 用 Rust 所有权串行化对 PIOC 的访问；
- 隐藏地址、命令号、buffer 布局和中断确认细节；
- 支持短命令、长 payload 和 IRQ streaming；
- 明确固件 ABI 版本、时钟 profile、pin 和 RAM 需求；
- 错误中保留 backend 错误和 PIOC 固件 status；
- 测试可以检查命令序列，也可以运行真实官方 BIN。

### 4.2 非目标

- 不把 WCH PIOC 假装成 RP PIO；
- 不承诺一个固件映像同时运行多个互不相关协议；
- 不在协议 crate 内绑定具体 NVIC/executor；
- 不把外部设备模型放入硬件 HAL；
- 不将未经许可确认的 WCH 二进制直接打包进可再分发 crate；
- 不把 VM 的数字结果描述成硅片级电气验证。

## 5. 建议的 crate 分层

```text
pioc-core
  Instruction / Register / ProgramImage 基础类型

pioc-parser + pioc-proc
  汇编、反汇编、编译期程序生成

pioc-hal-ch32x035                 （建议新增）
  寄存器、SRAM 装载、reset/run、IRQ future、pin remap

pioc-programs                    （原型已新增）
  backend-neutral protocol API
  ws2812 / ds18x20 / uart / i2c / nec / onewire-v2

pioc-vm
  指令、SFR、GPIO、Timer、codec、trace

pioc-vm-adapter                  （可并入 pioc-programs feature）
  CommandPort/EventPort for Machine
```

应用层只依赖 `pioc-programs` 和一个 backend，不需要知道 `d7` 或 `0x33` 的
具体含义。

## 6. 已实现的最小 backend 契约

原型位于 `pioc/pioc-programs/src/lib.rs`：

```rust
pub trait CommandPort {
    type Error;

    fn reset_halted(&mut self, pins: PinMask) -> Result<(), Self::Error>;
    fn run(&mut self, pins: PinMask) -> Result<(), Self::Error>;
    fn set_host_config(&mut self, enabled: bool) -> Result<(), Self::Error>;

    fn write_data(&mut self, offset: u8, data: &[u8])
        -> Result<(), Self::Error>;
    fn read_data(&mut self, offset: u8, data: &mut [u8])
        -> Result<(), Self::Error>;

    fn write_program_ram(&mut self, offset: u16, data: &[u8])
        -> Result<(), Self::Error>;
    fn program_ram_capacity(&self) -> usize;

    fn start_command(&mut self, command: u8) -> Result<(), Self::Error>;

    fn wait_for_interrupt(&mut self)
        -> impl Future<Output = Result<u8, Self::Error>> + '_;
}
```

### 6.1 为什么不是 FIFO trait

`wait_for_interrupt()` 对命令型固件表示 completion；对 UART/I²C streaming
固件则可以重复调用，每次返回后由协议驱动检查数据区 flag、refill buffer 或取出
RX 数据。这样不会把 `cmd.out` 错误抽象成 RP TX/RX FIFO。

### 6.2 为什么返回 `impl Future`

- 不需要 `async-trait` 或 heap allocation；
- 可以保持 `no_std`；
- 硬件 backend 返回 IRQ future；
- VM backend 可以返回“运行到 IRQ”的 future；
- 静态分发，符合嵌入式资源所有权模型。

该 trait 不是 object-safe，这是有意选择。PIOC 驱动通常在编译期知道 backend 类型。

### 6.3 后续应拆出的更低层接口

为了正确表达所有固件，建议第二阶段拆分为：

```rust
trait PiocControl {
    fn reset_halted(...);
    fn load_image(...);
    fn run(...);
    fn abort(...);
}

trait PiocRegisters {
    fn read_data(...);
    fn write_data(...);
    fn read_xchg(...);
    fn write_xchg(...);
    fn write_program_ram(...);
}

trait PiocEvents {
    fn start_command(...);
    fn wait_irq(...) -> impl Future<...>;
    fn read_command_out(...);
    fn acknowledge_irq(...);
}
```

当前 `CommandPort` 是经过验证的 MVP，拆分后可以完整支持 UART/I²C 的中间 IRQ，
并把“读 cmd.out 自动清 IRQ”和“伪写 cmd.out 清 IRQ”的芯片细节留给 backend。

## 7. 程序与 ABI 类型

Embassy 使用 `PioWs2812Program` 表示已经装入指令 RAM 的程序。PIOC 对应物不应
只有裸 `&[u8]`，建议定义：

```rust
pub trait ProgramSpec {
    const ABI: ProgramAbi;
    fn image(&self) -> &[u8];
}

pub struct ProgramAbi {
    pub name: &'static str,
    pub version: AbiVersion,
    pub required_clock: ClockRequirement,
    pub pins: PinMask,
    pub program_words: u16,
    pub payload_ram: Option<Range<u16>>,
    pub command_set: &'static [CommandDescriptor],
}

pub struct Loaded<B, P> {
    backend: B,
    program: P,
}
```

期望用法：

```rust
let pioc = PiocManager::new(peripheral, irq);
let loaded = pioc.load(Rgb1wProgram::new(RGB1W_BIN)).await?;
let mut leds = Ws2812::new(loaded, config);
```

`Loaded<B, Rgb1wProgram>` 的类型可以防止把 UART driver 绑定到 RGB1W 映像。

当前原型为了先验证协议层，构造器暂时假设调用者已经装载正确固件。正式硬件 API
应引入上述 typestate。

## 8. 错误模型

原型已经实现：

```rust
pub enum Error<E> {
    Backend(E),
    EmptyPayload,
    PayloadTooLong { actual: usize, maximum: usize },
    SfrCommandCollision { length: usize, command: u8 },
    ProgramRamUnavailable,
    InvalidBitCycles(u8),
    Firmware(FirmwareError),
}
```

官方 RGB1W status 被映射为：

| status | 类型化错误 |
|---:|---|
| `0` | success |
| `1` | `UnknownCommand` |
| `2` | `InvalidParameter` |
| `4` | `OutputHigh` |
| `6` | `InputHigh` |
| 其他 | `Other(code)` |

正式版还应增加：

- `Busy`；
- `Timeout`；
- `WrongProgramAbi`；
- `UnsupportedClock`；
- `PinUnavailable`；
- `CancelledNeedsRecovery`；
- streaming 的 overrun/underrun、parity/framing/NACK/arbitration 错误。

## 9. WS2812 API

### 9.1 当前 API

```rust
let config = ws2812::Config {
    output: OutputPin::Io0,
    storage: Storage::Auto,
    bit_cycles: 56,
};

let mut leds = Ws2812::new(port, config);

leds.write_rgb(
    &[Rgb8::new(255, 0, 0)],
    ColorOrder::Grb,
).await?;
```

也可直接发送预编码 byte stream：

```rust
leds.write_raw(&[0x00, 0xff, 0x00]).await?;
```

支持六种 RGB 排列：`RGB/RBG/GRB/GBR/BRG/BGR`。固件本身发送任意 byte，
因此 RGBW 或自定义灯珠可先用 `write_raw`，正式版再增加 `Rgbw8` 和颜色顺序 trait。

### 9.2 自动选择存储路径

`Storage::Auto` 使用以下规则：

1. payload 为空：报错；
2. 1..32 byte 且短命令无冲突：使用 SFR；
3. 其他长度：使用 program RAM；
4. RAM backend 不可用：`ProgramRamUnavailable`；
5. 超过 backend 容量或官方 3072-byte 上限：`PayloadTooLong`。

实现不需要 heap：`write_rgb(&[Rgb8])` 逐像素打包为局部 `[u8; 3]`，然后写 SFR
或停钟状态下的 program RAM。

### 9.3 IO1 的命令冲突

官方 short 模式用：

```text
IO0 command = length
IO1 command = 0x40 | length
```

而同一固件先解释：

```text
0x41 = DS1W convert
0x42 = DS1W read
0x43 = DS1W convert + read
```

因此 IO1 发送 1、2、3 byte 会碰撞。一个 RGB 灯正好是 3 byte，`0x43` 会误入
温度转换路径。原型行为：

- `Storage::Auto`：自动改用 RAM mode；
- `Storage::Sfr`：返回 `SfrCommandCollision`；
- 绝不静默发送错误命令。

这是高层 API 比官方 C 包装更安全的直接例子。

### 9.4 时钟

短模式的软件延时依赖固件映像中的 frequency patch；长模式命令低 7 位直接传
`bit_cycles`。官方值：

| PIOC/system clock | bit cycles |
|---:|---:|
| 48 MHz | 56 |
| 24 MHz | 28 |

正式 `ProgramSpec` 应在 load 阶段应用短模式 patch，并根据实际时钟计算/验证
long-mode bit cycles。当前原型要求用户正确配置 `bit_cycles`，并限制到固件接受的
`8..=124`。

## 10. OneWire 与 DS18x20

### 10.1 不能直接复制 Embassy `PioOneWire`

Embassy RP 实现提供：

```text
reset
write_bytes
read_bytes
write_bytes_pullup
ROM search
```

官方 WCH RGB1W 固件只导出：

```text
0x41 convert
0x42 read first two scratchpad bytes
0x43 convert, wait 750 ms, read
```

内部固件确实有 reset/read/write 子程序，但没有通过 host ABI 暴露。因此当前
`pioc-programs` 正确命名为 `Ds18x20`，而不是声称实现通用 `OneWireBus`。

### 10.2 当前 DS18x20 API

```rust
let mut sensor = Ds18x20::new(port);

sensor.start_conversion(PowerMode::Parasite).await?;
// 主机等待传感器转换时间
let temperature = sensor.read_temperature().await?;

let milli_c = temperature.milli_celsius();
```

也支持固件内部阻塞 750 ms 的命令：

```rust
let temperature = sensor
    .convert_and_read(PowerMode::External)
    .await?;
```

`PowerMode::Parasite` 映射到 `sys.ctrl.host_config`，固件在发送 `44h` 后主动推高
IO0 提供强上拉。

### 10.3 通用 OneWire V2 固件建议

若要获得 Embassy 同等级能力，需要定义新 PIOC firmware ABI：

```text
01h RESET                  -> presence/status
02h WRITE                  d0=len, d1.. payload
03h READ                   d0=len, result in data/RAM
04h TRIPLET                search direction -> id/complement/result
05h STRONG_PULLUP_BEGIN
06h STRONG_PULLUP_END
07h TOUCH_BITS             simultaneous write/read slots
```

对应 API：

```rust
trait OneWireBus {
    async fn reset(&mut self) -> Result<bool, Error>;
    async fn write(&mut self, bytes: &[u8]) -> Result<(), Error>;
    async fn read(&mut self, bytes: &mut [u8]) -> Result<(), Error>;
    async fn triplet(&mut self, direction: bool) -> Result<Triplet, Error>;
}
```

ROM search 应放在 Rust 协议层，PIOC 只执行精确定时的 triplet；这比把完整 search
状态机塞入 PIOC 更容易测试，也允许多个传感器。

## 11. UART 与 I²C streaming API

### 11.1 UART

官方 UART 使用两组 8-byte TX buffer 交替 refill，IRQ 可能表示：

- RX byte ready；
- TX buffer A 可补充；
- TX buffer B 可补充；
- 发送完成或错误。

建议 API：

```rust
pub struct PiocUart<B> { /* owns backend and buffer state */ }

impl<B> PiocUart<B> {
    pub fn split(self) -> (UartTx<B>, UartRx<B>);
    pub fn set_config(&mut self, config: UartConfig) -> Result<(), Error>;
}

impl embedded_io_async::Write for UartTx<_> { ... }
impl embedded_io_async::Read for UartRx<_> { ... }
```

需要注意：单个 PIOC backend 无法像 RP 的两个 state machine 那样被两个所有者直接
拆开。实现应使用共享内部状态和唯一 IRQ dispatcher，或者在一个 `run()` task 中
维护 TX/RX ring buffers，再向应用暴露 pipe/channel handle。

### 11.2 I²C

建议分别实现 controller 与 target：

```rust
PiocI2cController<B>
    write(address, bytes)
    read(address, bytes)
    write_read(address, tx, rx)

PiocI2cTarget<B>
    run(handler).await
```

controller 可实现 `embedded-hal-async::i2c::I2c`。target 需要持续 IRQ service，不能
建模成单次 `command().await`。

## 12. 所有权、并发与取消

### 12.1 独占所有权

协议 driver 持有 backend：

```rust
pub struct Ws2812<B> {
    port: B,
    config: Config,
}
```

所有方法使用 `&mut self`，因此编译器阻止两个命令同时修改同一 PIOC。`release()`
归还 backend，允许应用显式切换固件/协议。

### 12.2 取消安全

`write_rgb().await` 在 `cmd.in` 写入后若 future 被取消，PIOC 仍会继续运行。这与
DMA/FIFO 驱动类似，不能声称 cancellation-safe。

正式版应提供：

```rust
driver.is_busy();
driver.wait_idle().await;
driver.abort().await;       // reset/halt and restore known state
driver.recover().await;     // reload/restart firmware when required
```

文档必须标注：丢弃操作 future 不等于取消硬件事务。

### 12.3 超时

协议 crate 不应绑定 `embassy_time::Duration`。两种方案：

- 由应用使用 executor 的 `with_timeout` 包裹 future；
- backend 提供可选 deadline/timeout adapter。

超时后调用 `abort/recover`，避免下一次操作把旧 IRQ 当作新 completion。

## 13. Hardware backend 草图

CH32X035 backend 的职责：

```rust
struct Ch32x035Pioc<'d> {
    regs: PeripheralRef<'d, PIOC>,
    irq_state: &'d IrqState,
    payload_ram_capacity: usize,
}
```

实现顺序：

1. 配置 AFIO/GPIO remap；
2. `reset_halted` 写 `sys.ctrl` reset/ownership；
3. 停钟时复制 program image 或 long payload 到共享 SRAM；
4. `run` 释放 reset 并开启 clock；
5. `write_data/read_data` 使用 volatile SFR；
6. `start_command` 最后写 `cmd.in`，形成 release boundary；
7. IRQ handler 只清 NVIC source、记录事件并 wake；
8. future 被 poll 后读取/确认 PIOC IRQ 和 `cmd.out`。

需要通过 critical section 保证：

- command payload 全部写完后才写 `cmd.in`；
- IRQ handler 与任务不会同时 refill 同一 buffer；
- RAM mode 写 SRAM 时 PIOC 确实停钟；
- 同周期主机/PIOC SFR 写入遵循硬件“host wins”规则。

## 14. VM backend

VM backend 应完成同一 trait：

| CommandPort 方法 | VM 行为 |
|---|---|
| `reset_halted` | host write `sys.ctrl` reset/claims |
| `run` | host write clock-run/claims |
| `write_data` | host write `d0..d31` |
| `write_program_ram` | 写 VM 暴露的共享程序 SRAM窗口 |
| `start_command` | host write `cmd.in` |
| `wait_for_interrupt` | 执行 VM，直到 `interrupt_request` |

当前端到端测试在测试文件中实现了最小 VM adapter，覆盖 SFR WS2812 路径。VM 尚未
公开“停钟时主机写 program SRAM”的接口，因此 long/RAM path 当前只由 mock 验证。
下一步应给 `Machine` 增加受 clock-gate/reset 约束的 host program-memory API，而不是
让 adapter 直接修改私有 `program` 数组。

## 15. 可行性验证

### 15.1 编译属性

- `pioc-programs` 使用 `#![no_std]`；
- 生产依赖为空；
- 不需要 allocator；
- async trait 边界不需要装箱；
- workspace 禁止 unsafe code 的规则仍生效。

### 15.2 Mock 行为测试

已验证：

1. 一个纯红灯被编码为 `00 FF 00`；
2. IO0 3-byte 自动选择 SFR command `03h`；
3. IO1 3-byte 自动切换 RAM mode；
4. 强制 IO1 SFR mode 返回 `SfrCommandCollision(0x43)`；
5. RAM mode 写 payload、长度、pin 和 `0x80|bit_cycles`；
6. DS18x20 命令 `41h/42h` 和 parasite flag 映射正确；
7. 有符号温度 raw value 转换为 milli-Celsius。

### 15.3 官方 BIN 端到端测试

测试加载官方 `RGB1W.BIN` 到 `pioc-vm::Machine`，然后只通过高层 API 调用：

```rust
driver
    .write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)
    .await?;
```

VM trace 的 IO0 高脉宽被重新解码为：

```text
00000000 11111111 00000000
   G        R        B
```

测试没有直接写 `d0`、`cmd.in` 或 PC，证明高层 API 到官方 firmware ABI 的闭环成立。

### 15.4 尚未验证

- CH32X035 真机 IRQ future；
- RAM long-mode 的 VM/真机端到端路径；
- IO1 long-mode 真机输出；
- DS18x20 presence、读取与 parasite strong pull-up 的主动外设模型；
- UART/I²C 长数据 streaming；
- 24 MHz firmware patch；
- 取消、超时和中断竞争条件。

## 16. 性能和容量

### 16.1 WS2812

| 路径 | 最大数据 | RGB 灯数量 | 主机参与 |
|---|---:|---:|---|
| SFR | 32 byte | 10 灯 + 2 byte | 一次写共享 SFR |
| RAM | 3072 byte | 1024 灯 | 停钟时一次/分块写 SRAM |

RAM mode 不应在每个像素后 run/halt；原型先停钟，连续写所有像素，再一次启动。

### 16.2 UART/I²C

官方双缓冲意味着可实现零 heap streaming，但 IRQ service 的最坏延迟必须小于一个
buffer 的线速耗尽时间。API 文档应提供：

```text
service_budget = buffer_bytes * bits_per_byte / baud
```

高波特率下可选择：

- 提高 IRQ 优先级；
- 使用更大的共享 SRAM 协议；
- 若芯片支持合适 DMA 路由，再在硬件 backend 内加 DMA；
- 不应在通用协议层承诺 PIOC 本身具有 RP 风格 DMA FIFO。

## 17. 建议的落地阶段

### Phase 1：已完成原型

- `CommandPort`；
- 类型化 error/status；
- `Ws2812` SFR/RAM 选择；
- `Ds18x20` legacy API；
- mock 测试；
- 官方 RGB1W BIN 的 VM 端到端测试。

### Phase 2：硬件与 VM backend

- `ProgramSpec`、`ProgramAbi`、`Loaded<B,P>`；
- CH32X035 volatile register/SRAM backend；
- Embassy-compatible IRQ future；
- VM host program-RAM API；
- timeout、abort、recover；
- 真机逻辑分析仪校准 24/48 MHz。

### Phase 3：streaming

- 拆分 `PiocControl/PiocRegisters/PiocEvents`；
- UART TX/RX 双缓冲状态机；
- `embedded-io-async` traits；
- I²C controller `embedded-hal-async` trait；
- target/IRQ service task；
- overrun、framing、NACK 等错误。

### Phase 4：通用 OneWire 和协议生态

- 新 OneWire V2 firmware ABI；
- reset/read/write/triplet/strong-pullup；
- ROM search 与 DS18B20 scratchpad/CRC；
- NEC、single-wire debug、I²C target；
- 硬件 backend 与 VM peer-model 的共用协议测试向量。

## 18. 最终建议

`pioc-programs` 应采用 Embassy 的“Program + owned Driver + async operation”思想，
但以 WCH 的 command/SFR/IRQ ABI 为核心。近期最合理的公开 API 是：

```text
ProgramSpec -> Loaded<B, Program>
             -> Ws2812 / Ds18x20 / Uart / I2c
             -> async protocol methods
```

不要公开要求用户手工填写 `d0`、`d7`、`cmd.in=0x33` 的 API，也不要用一个过度
抽象的通用 FIFO 掩盖不同固件的 buffer ABI。协议 driver 应成为固件 ABI 的唯一
解释者；backend 只负责可靠地装载、读写、等待 IRQ 和恢复硬件。

现有原型已经证明该方向在 Rust 类型系统、`no_std`、官方 RGB1W 固件和 VM 上均可行。
下一项最高价值工作是实现正式 `ProgramSpec/Loaded` typestate 与 CH32X035/VM 两个
backend，然后再以 UART 双缓冲验证 streaming 事件模型。
