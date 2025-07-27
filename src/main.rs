use eframe::egui;
use egui::{Color32, RichText, Vec2};
use enigo::{Enigo, MouseButton, MouseControllable};
use rand::Rng;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use hotkey::Listener;
use std::fs;
use std::path::PathBuf;
// use rfd::FileDialog;
use ctrlc;

// Click Mode Enum
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
enum ClickMode {
    FixedInterval,   // Fixed Interval Click
    RandomInterval,  // Random Interval Click
    Continuous,      // Continuous Click (as fast as possible)
    Pattern,         // Pattern Click (custom sequence)
}

impl ClickMode {
    fn name(&self) -> &'static str {
        match self {
            ClickMode::FixedInterval => "Fixed Interval",
            ClickMode::RandomInterval => "Random Interval",
            ClickMode::Continuous => "Continuous",
            ClickMode::Pattern => "Pattern",
        }
    }
    
    fn all() -> Vec<ClickMode> {
        vec![ClickMode::FixedInterval, ClickMode::RandomInterval, ClickMode::Continuous, ClickMode::Pattern]
    }
}

// Mouse Button Enum
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
enum MouseButtonType {
    Left,
    Right,
    Middle,
}

impl MouseButtonType {
    fn name(&self) -> &'static str {
        match self {
            MouseButtonType::Left => "Left",
            MouseButtonType::Right => "Right",
            MouseButtonType::Middle => "Middle",
        }
    }
    
    fn to_enigo_button(&self) -> MouseButton {
        match self {
            MouseButtonType::Left => MouseButton::Left,
            MouseButtonType::Right => MouseButton::Right,
            MouseButtonType::Middle => MouseButton::Middle,
        }
    }
    
    fn all() -> Vec<MouseButtonType> {
        vec![MouseButtonType::Left, MouseButtonType::Right, MouseButtonType::Middle]
    }
}

// Configuration Structure for Save and Load
#[derive(Serialize, Deserialize, Clone)]
struct ClickerConfig {
    name: String,
    click_mode: ClickMode,
    mouse_button: MouseButtonType,
    fixed_interval_ms: u64,
    min_random_interval_ms: u64,
    max_random_interval_ms: u64,
    pattern_intervals: Vec<u64>,
}

impl Default for ClickerConfig {
    fn default() -> Self {
        Self {
            name: "默认配置".to_string(),
            click_mode: ClickMode::FixedInterval,
            mouse_button: MouseButtonType::Left,
            fixed_interval_ms: 100,
            min_random_interval_ms: 50,
            max_random_interval_ms: 200,
            pattern_intervals: vec![100, 200, 300],
        }
    }
}

// Clicker Status
struct ClickerState {
    is_running: bool,
    click_mode: ClickMode,
    mouse_button: MouseButtonType,
    fixed_interval_ms: u64,
    min_random_interval_ms: u64,
    max_random_interval_ms: u64,
    pattern_intervals: Vec<u64>,
    click_count: u64,
    start_time: Option<Instant>,
    last_click_time: Option<Instant>,
}

impl Default for ClickerState {
    fn default() -> Self {
        Self {
            is_running: false,
            click_mode: ClickMode::FixedInterval,
            mouse_button: MouseButtonType::Left,
            fixed_interval_ms: 100,
            min_random_interval_ms: 50,
            max_random_interval_ms: 200,
            pattern_intervals: vec![100, 200, 300],
            click_count: 0,
            start_time: None,
            last_click_time: None,
        }
    }
}

impl From<&ClickerConfig> for ClickerState {
    fn from(config: &ClickerConfig) -> Self {
        Self {
            is_running: false,
            click_mode: config.click_mode,
            mouse_button: config.mouse_button,
            fixed_interval_ms: config.fixed_interval_ms,
            min_random_interval_ms: config.min_random_interval_ms,
            max_random_interval_ms: config.max_random_interval_ms,
            pattern_intervals: config.pattern_intervals.clone(),
            click_count: 0,
            start_time: None,
            last_click_time: None,
        }
    }
}

// Hotkey Status
static HOTKEY_ACTIVE: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));

// Hotkey Command Channel
static HOTKEY_COMMAND: Lazy<Arc<Mutex<Option<bool>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

// Config File Path
fn get_config_dir() -> PathBuf {
    let path = if let Some(proj_dirs) = directories::ProjectDirs::from("com", "SeriousClick", "SeriousClick") {
        proj_dirs.config_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".config")
    };
    
    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }
    
    path.join("configs.json")
}

// Application State
struct SeriousClickerApp {
    state: Arc<Mutex<ClickerState>>,
    clicker_thread: Option<thread::JoinHandle<()>>,
    configs: Vec<ClickerConfig>,
    selected_config_index: usize,
    editing_config: ClickerConfig,
    is_editing: bool,
    pattern_input: String,
    status_message: String,
    hotkey_active: bool,
}

impl SeriousClickerApp {
    fn new() -> Self {
        let configs = Self::load_configs().unwrap_or_else(|_| vec![ClickerConfig::default()]);
        let default_config = configs.first().cloned().unwrap_or_default();
        let state = ClickerState::from(&default_config);
        
        let pattern_input = default_config.pattern_intervals
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        
        Self {
            state: Arc::new(Mutex::new(state)),
            clicker_thread: None,
            configs,
            selected_config_index: 0,
            editing_config: default_config,
            is_editing: false,
            pattern_input,
            status_message: "准备就绪".to_string(),
            hotkey_active: false,
        }
    }
    
    fn load_configs() -> Result<Vec<ClickerConfig>, Box<dyn std::error::Error>> {
        let config_path = get_config_dir();
        if !config_path.exists() {
            return Ok(vec![ClickerConfig::default()]);
        }
        
        let config_str = fs::read_to_string(config_path)?;
        let configs: Vec<ClickerConfig> = serde_json::from_str(&config_str)?;
        Ok(configs)
    }
    
    fn save_configs(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = get_config_dir();
        let config_str = serde_json::to_string_pretty(&self.configs)?;
        fs::write(config_path, config_str)?;
        Ok(())
    }
    
    fn apply_config(&mut self, config: ClickerConfig) {
        let mut state = self.state.lock().unwrap();
        state.click_mode = config.click_mode;
        state.mouse_button = config.mouse_button;
        state.fixed_interval_ms = config.fixed_interval_ms;
        state.min_random_interval_ms = config.min_random_interval_ms;
        state.max_random_interval_ms = config.max_random_interval_ms;
        state.pattern_intervals = config.pattern_intervals.clone();
        
        self.pattern_input = config.pattern_intervals
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
    }

    fn toggle_clicker(&mut self) {
        let is_running = {
            let state = self.state.lock().unwrap();
            state.is_running
        };
        
        if is_running {
            self.stop_clicker();
        } else {
            self.start_clicker();
        }
    }
    
    fn start_clicker(&mut self) {
        let mut state = self.state.lock().unwrap();
        if state.is_running {
            return; // 已经在运行了
        }
        
        state.is_running = true;
        state.start_time = Some(Instant::now());
        state.click_count = 0;
        drop(state);
        
        self.status_message = "连点器已启动".to_string();
        
        // 创建点击线程
        let state_clone = Arc::clone(&self.state);
        self.clicker_thread = Some(thread::spawn(move || {
            let mut enigo = Enigo::new();
            let mut pattern_index = 0;
            
            loop {
                let (should_continue, delay, button, mode, pattern) = {
                    let state = state_clone.lock().unwrap();
                    if !state.is_running {
                        break;
                    }
                    
                    let button = state.mouse_button.to_enigo_button();
                    let mode = state.click_mode;
                    
                    let delay = match mode {
                        ClickMode::FixedInterval => state.fixed_interval_ms,
                        ClickMode::RandomInterval => {
                            let mut rng = rand::thread_rng();
                            rng.gen_range(state.min_random_interval_ms..=state.max_random_interval_ms)
                        },
                        ClickMode::Continuous => 1, // 尽可能快的点击
                        ClickMode::Pattern => {
                            if state.pattern_intervals.is_empty() {
                                100 // 默认值
                            } else {
                                state.pattern_intervals[pattern_index]
                            }
                        },
                    };
                    
                    let pattern = state.pattern_intervals.clone();
                    (true, delay, button, mode, pattern)
                };
                
                if should_continue {
                    // 执行点击
                    enigo.mouse_click(button);
                    
                    // 更新状态
                    let mut state = state_clone.lock().unwrap();
                    state.click_count += 1;
                    state.last_click_time = Some(Instant::now());
                    drop(state);
                    
                    // 更新模式索引
                    if mode == ClickMode::Pattern && !pattern.is_empty() {
                        pattern_index = (pattern_index + 1) % pattern.len();
                    }
                    
                    // 等待下一次点击
                    thread::sleep(Duration::from_millis(delay));
                }
            }
        }));
    }
    
    fn stop_clicker(&mut self) {
        let mut state = self.state.lock().unwrap();
        if !state.is_running {
            return; // 已经停止了
        }
        
        state.is_running = false;
        drop(state);
        
        self.status_message = "连点器已停止".to_string();
        
        if let Some(handle) = self.clicker_thread.take() {
            // 线程会自行结束，因为我们已经设置了is_running = false
            let _ = handle.join();
        }
    }
    
    fn setup_hotkey(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 如果热键已经激活，不需要再次设置
        if self.hotkey_active {
            return Ok(());
        }
        
        // 设置热键状态
        *HOTKEY_ACTIVE.lock().unwrap() = true;
        
        // 在新线程中启动热键监听
        thread::spawn(move || {
            let mut listener = Listener::new();
            
            // 注册F8热键用于开始/停止连点
            // 根据hotkey库0.3.1版本，使用0作为modifiers表示没有修饰键
            // 对于F8键，使用虚拟键码0x77（十进制119）
            if let Ok(_) = listener.register_hotkey(
                0, // 替换modifiers::NONE
                0x77, // F8键的虚拟键码
                move || {
                    // 通过命令通道发送切换命令
                    let mut command = HOTKEY_COMMAND.lock().unwrap();
                    *command = Some(true); // 设置为Some(true)表示需要切换状态
                },
            ) {
                // 开始监听热键
                listener.listen();
            }
        });
        
        self.hotkey_active = true;
        self.status_message = "热键已激活: F8 = 开始/停止".to_string();
        
        Ok(())
    }
    
    fn get_status_text(&self) -> String {
        let state = self.state.lock().unwrap();
        let mut status = format!("状态: {}", if state.is_running { "运行中" } else { "已停止" });
        
        if let Some(start_time) = state.start_time {
            let elapsed = start_time.elapsed();
            status.push_str(&format!(" | 运行时间: {}分{}秒", elapsed.as_secs() / 60, elapsed.as_secs() % 60));
        }
        
        status.push_str(&format!(" | 点击次数: {}", state.click_count));
        
        if let Some(last_time) = state.last_click_time {
            status.push_str(&format!(" | 上次点击: {}毫秒前", last_time.elapsed().as_millis()));
        }
        
        status
    }


}

// 实现eframe的App trait
impl eframe::App for SeriousClickerApp {
    
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 检查热键命令
        {
            let mut command = HOTKEY_COMMAND.lock().unwrap();
            if command.is_some() {
                // 收到热键命令，切换连点器状态
                self.toggle_clicker();
                *command = None; // 重置命令
            }
        }
        
        // 设置视觉风格
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.window_margin = egui::Margin::same(10.0);
        style.visuals.window_fill = Color32::from_rgb(32, 32, 32);
        style.visuals.panel_fill = Color32::from_rgb(32, 32, 32);
        ctx.set_style(style);
        // 更新状态文本
        let status_text = self.get_status_text();
        
        // 顶部菜单栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("文件", |ui| {
                    if ui.button("保存配置").clicked() {
                        if let Err(err) = self.save_configs() {
                            self.status_message = format!("保存配置失败: {}", err);
                        } else {
                            self.status_message = "配置已保存".to_string();
                        }
                        ui.close_menu();
                    }
                    
                    if ui.button("退出").clicked() {
                        self.stop_clicker();
                        std::process::exit(0);
                    }
                });
                
                ui.menu_button("热键", |ui| {
                    if ui.button(if self.hotkey_active { "禁用热键" } else { "启用热键" }).clicked() {
                        if self.hotkey_active {
                            // 禁用热键
                            *HOTKEY_ACTIVE.lock().unwrap() = false;
                            self.hotkey_active = false;
                            self.status_message = "热键已禁用".to_string();
                        } else {
                            // 启用热键
                            if let Err(err) = self.setup_hotkey() {
                                self.status_message = format!("设置热键失败: {}", err);
                            }
                        }
                        ui.close_menu();
                    }
                });
                
                ui.menu_button("帮助", |ui| {
                    if ui.button("关于").clicked() {
                        self.status_message = "SeriousClick 专业连点器 v0.1.0".to_string();
                        ui.close_menu();
                    }
                });
            });
        });
        
        // 底部状态栏
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&status_text).text_style(egui::TextStyle::Monospace));
                ui.separator();
                ui.label(RichText::new(&self.status_message).text_style(egui::TextStyle::Monospace));
                
                if self.hotkey_active {
                    ui.separator();
                    ui.label(RichText::new("热键: F8 = 开始/停止").text_style(egui::TextStyle::Monospace));
                }
            });
        });
        
        // 主界面
        egui::CentralPanel::default().show(ctx, |ui| {
            // 配置列表和控制按钮
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(if self.state.lock().unwrap().is_running { "停止 ⏹" } else { "开始 ▶" })
                    .min_size(Vec2::new(100.0, 30.0)))
                    .clicked() 
                {
                    self.toggle_clicker();
                }
                
                ui.separator();
                
                egui::ComboBox::from_label("配置")
                    .selected_text(if self.configs.is_empty() {
                        "无配置".to_string()
                    } else {
                        self.configs[self.selected_config_index].name.clone()
                    })
                    .show_ui(ui, |ui| {
                        let mut config_to_apply = None;
                        for (i, config) in self.configs.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_config_index, i, &config.name).clicked() {
                                config_to_apply = Some(config.clone());
                            }
                        }
                        
                        if let Some(config) = config_to_apply {
                            self.apply_config(config);
                        }
                    });
                
                if ui.button("New").clicked() {
                    self.editing_config = ClickerConfig::default();
                    self.editing_config.name = format!("Config {}", self.configs.len() + 1);
                    self.is_editing = true;
                }
                
                if !self.configs.is_empty() {
                    if ui.button("Edit").clicked() {
                        self.editing_config = self.configs[self.selected_config_index].clone();
                        self.is_editing = true;
                    }
                    
                    if ui.button("Delete").clicked() && !self.configs.is_empty() {
                        self.configs.remove(self.selected_config_index);
                        if self.configs.is_empty() {
                            self.configs.push(ClickerConfig::default());
                        }
                        self.selected_config_index = self.selected_config_index.min(self.configs.len() - 1);
                        self.apply_config(self.configs[self.selected_config_index].clone());
                        let _ = self.save_configs();
                    }
                }
            });
            
            ui.add_space(10.0);
            
            // 配置表格
            egui::Frame::dark_canvas(ui.style()).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("clicker_config_grid")
                        .striped(true)
                        .spacing([10.0, 5.0])
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            // 表头
                            ui.label(RichText::new("No.").strong());
                            ui.label(RichText::new("Window Title").strong());
                            ui.label(RichText::new("Click Type").strong());
                            ui.label(RichText::new("Interval").strong());
                            ui.end_row();
                            
                            // 配置行
                            for (i, config) in self.configs.iter().enumerate() {
                                let is_selected = i == self.selected_config_index;
                                let row_color = if is_selected { Color32::from_rgb(60, 100, 150) } else { ui.style().visuals.widgets.noninteractive.bg_fill };
                                
                                ui.scope(|ui| {
                                    ui.style_mut().visuals.widgets.noninteractive.bg_fill = row_color;
                                    ui.label(RichText::new(format!("{}", i + 1)).strong());
                                    ui.label(&config.name);
                                    ui.label(config.click_mode.name());
                                    
                                    let interval_text = match config.click_mode {
                                        ClickMode::FixedInterval => format!("{} ms", config.fixed_interval_ms),
                                        ClickMode::RandomInterval => format!("{}-{} ms", config.min_random_interval_ms, config.max_random_interval_ms),
                                        ClickMode::Continuous => "Continuous".to_string(),
                                        ClickMode::Pattern => {
                                            let intervals = config.pattern_intervals.iter()
                                                .map(|i| i.to_string())
                                                .collect::<Vec<_>>()
                                                .join(",");
                                            format!("[{}] ms", intervals)
                                        }
                                    };
                                    ui.label(interval_text);
                                });
                                ui.end_row();
                            }
                        });
                });
            });
            
            // 编辑配置对话框
            if self.is_editing {
                egui::Window::new("Edit Configuration")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Config Name:");
                            ui.text_edit_singleline(&mut self.editing_config.name);
                        });
                        
                        ui.add_space(5.0);
                        
                        ui.horizontal(|ui| {
                            ui.label("Click Mode:");
                            egui::ComboBox::from_id_source("click_mode")
                                .selected_text(self.editing_config.click_mode.name())
                                .show_ui(ui, |ui| {
                                    for mode in ClickMode::all() {
                                        ui.selectable_value(&mut self.editing_config.click_mode, mode, mode.name());
                                    }
                                });
                        });
                        
                        ui.add_space(5.0);
                        
                        ui.horizontal(|ui| {
                            ui.label("Mouse Button:");
                            egui::ComboBox::from_id_source("mouse_button")
                                .selected_text(self.editing_config.mouse_button.name())
                                .show_ui(ui, |ui| {
                                    for button in MouseButtonType::all() {
                                        ui.selectable_value(&mut self.editing_config.mouse_button, button, button.name());
                                    }
                                });
                        });
                        
                        ui.add_space(5.0);
                        
                        // 根据点击模式显示不同的配置选项
                        match self.editing_config.click_mode {
                            ClickMode::FixedInterval => {
                                ui.horizontal(|ui| {
                                    ui.label("Click Interval (ms):");
                                    ui.add(egui::Slider::new(&mut self.editing_config.fixed_interval_ms, 1..=1000));
                                });
                            },
                            ClickMode::RandomInterval => {
                                ui.horizontal(|ui| {
                                    ui.label("Min Interval (ms):");
                                    ui.add(egui::Slider::new(&mut self.editing_config.min_random_interval_ms, 1..=500));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Max Interval (ms):");
                                    ui.add(egui::Slider::new(&mut self.editing_config.max_random_interval_ms, 
                                                            self.editing_config.min_random_interval_ms..=1000));
                                });
                            },
                            ClickMode::Pattern => {
                                ui.horizontal(|ui| {
                                    ui.label("Click Interval Sequence (ms, comma separated):");
                                    ui.text_edit_singleline(&mut self.pattern_input);
                                });
                            },
                            _ => {}
                        }
                        
                        ui.add_space(10.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                // 处理模式点击间隔
                                if self.editing_config.click_mode == ClickMode::Pattern {
                                    let mut intervals = Vec::new();
                                    for part in self.pattern_input.split(',') {
                                        if let Ok(interval) = part.trim().parse::<u64>() {
                                            intervals.push(interval);
                                        }
                                    }
                                    if !intervals.is_empty() {
                                        self.editing_config.pattern_intervals = intervals;
                                    }
                                }
                                
                                // 保存配置
                                if self.selected_config_index < self.configs.len() {
                                    self.configs[self.selected_config_index] = self.editing_config.clone();
                                } else {
                                    self.configs.push(self.editing_config.clone());
                                    self.selected_config_index = self.configs.len() - 1;
                                }
                                
                                // 应用配置
                                self.apply_config(self.editing_config.clone());
                                
                                // 保存到文件
                                let _ = self.save_configs();
                                
                                self.is_editing = false;
                            }
                            
                            if ui.button("Cancel").clicked() {
                                self.is_editing = false;
                            }
                        });
                    });
            }
        });

        // Request a repaint to ensure the UI is continuously updated
        ctx.request_repaint();
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(Vec2::new(800.0, 600.0)),
        resizable: true,
        ..Default::default()
    };
    
    // 设置Ctrl+C处理，确保在应用被强制关闭时也能清理热键
    ctrlc::set_handler(|| {
        // 禁用热键
        *HOTKEY_ACTIVE.lock().unwrap() = false;
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");
    
    let result = eframe::run_native(
        "SeriousClick Auto Clicker",
        options,
        Box::new(|cc| {
            // 设置UI比例
            cc.egui_ctx.set_pixels_per_point(1.25); // 增加UI比例，使文字更清晰
            
            // 使用默认字体配置
            let mut fonts = egui::FontDefinitions::default();
            
            // 设置中文字体范围
            let mut font_data = egui::FontData::from_static(include_bytes!("C:\\Windows\\Fonts\\simhei.ttf"));
            // 设置字体支持的字符范围
            font_data.tweak.scale = 1.0;
            
            // 添加中文字体
            fonts.font_data.insert("simhei".to_owned(), font_data);
            
            // 将中文字体设置为首选字体
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "simhei".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "simhei".to_owned());
            
            // 不添加不存在的备用字体，使用默认字体
            
            cc.egui_ctx.set_fonts(fonts);
            
            // 设置样式
            let mut style = (*cc.egui_ctx.style()).clone();
            style.text_styles = [
                (egui::TextStyle::Heading, egui::FontId::new(24.0, egui::FontFamily::Proportional)),
                (egui::TextStyle::Body, egui::FontId::new(18.0, egui::FontFamily::Proportional)),
                (egui::TextStyle::Monospace, egui::FontId::new(16.0, egui::FontFamily::Monospace)),
                (egui::TextStyle::Button, egui::FontId::new(18.0, egui::FontFamily::Proportional)),
                (egui::TextStyle::Small, egui::FontId::new(14.0, egui::FontFamily::Proportional)),
            ].into();
            cc.egui_ctx.set_style(style);
            
            Box::new(SeriousClickerApp::new())
        })
    );
    
    // 确保在应用退出时禁用热键
    *HOTKEY_ACTIVE.lock().unwrap() = false;
    
    result
}
