use std::{
    collections::VecDeque,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    time::Instant,
};

use fyrox::{
    asset::{event, io::FsResourceIo, manager::ResourceManager},
    core::{
        log::{LogMessage, MessageKind},
        pool::Handle,
        task::TaskPool,
    },
    dpi::{PhysicalSize, Size},
    engine::{
        ApplicationLoopController, Engine, EngineInitParams, GraphicsContext,
        GraphicsContextParams, SerializationContext,
    },
    event_loop::ActiveEventLoop,
    fxhash::{FxHashMap, FxHashSet},
    gui::{
        border::BorderBuilder,
        check_box::{CheckBoxBuilder, CheckBoxMessage},
        constructor::new_widget_constructor_container,
        font::Font,
        grid::GridBuilder,
        message::{MessageDirection, UiMessage},
        scroll_viewer::ScrollViewerBuilder,
        stack_panel::StackPanelBuilder,
        style::{resource::StyleResourceExt, Style, StyledProperty},
        text::TextBuilder,
        utils::make_image_button_with_tooltip,
        widget::{WidgetBuilder, WidgetMessage},
        BuildContext, HorizontalAlignment, Orientation, Thickness, UiNode, UserInterface,
        VerticalAlignment,
    },
    scene::graph::GraphUpdateSwitches,
    window::WindowAttributes,
};

use crate::{message::MessageSender, settings::Settings, GameLoopData, Message, FIXED_TIMESTEP};

pub struct LogChildOsWindow {
    // graphics_context: GraphicsContext,
    pub(crate) engine: Engine,
    pub(crate) log_message_receiver: Receiver<LogMessage>,
    pub(crate) message_sender: MessageSender,
    pub(crate) message_receiver: Receiver<Message>,
    pub(crate) game_loop_data: GameLoopData,
    pub(crate) root_grid: Handle<UiNode>,
    pub(crate) info_checkbox: Handle<UiNode>,
    pub(crate) warning_checkbox: Handle<UiNode>,
    pub(crate) error_checkbox: Handle<UiNode>,
    pub(crate) engine_checkbox: Handle<UiNode>,
    pub(crate) game_checkbox: Handle<UiNode>,
    pub(crate) info_checked: bool,
    pub(crate) warning_checked: bool,
    pub(crate) error_checked: bool,
    pub(crate) engine_checked: bool,
    pub(crate) game_checked: bool,
    pub(crate) log_stack_panel: Handle<UiNode>,
    pub(crate) messages_from_engine: VecDeque<LogMessage>,
    pub(crate) messages_from_game: VecDeque<LogMessage>,
    pub(crate) max_messages: usize,
    pub(crate) graphics_context_initialized_once: bool,
}

impl LogChildOsWindow {
    pub fn new(log_message_receiver: Receiver<LogMessage>) -> Self {
        let mut window_attributes = WindowAttributes::default();
        window_attributes.resizable = true;
        window_attributes.title = "Debug Log".to_string();
        window_attributes.inner_size = Some(Size::Physical(PhysicalSize::new(600, 800)));

        let graphics_context_params = GraphicsContextParams {
            window_attributes,
            vsync: true,
            msaa_sample_count: None,
            graphics_server_constructor: Default::default(),
            named_objects: false,
        };
        let task_pool = Arc::new(TaskPool::new());
        let serialization_context = Arc::new(SerializationContext::new());
        let mut engine = Engine::new(EngineInitParams {
            graphics_context_params,
            resource_manager: ResourceManager::new(Arc::new(FsResourceIo), task_pool.clone()),
            serialization_context,
            task_pool,
            widget_constructors: Arc::new(new_widget_constructor_container()),
        })
        .unwrap();

        let (message_sender, message_receiver) = mpsc::channel();
        let message_sender = MessageSender(message_sender);
        // don't know if this is necessary
        // {
        //     let mut font_state = engine.user_interfaces.first_mut().default_font.state();
        //     let font_state_data = font_state.data().unwrap();
        //     *font_state_data = Font::from_memory(
        //         include_bytes!("../resources/Roboto-Regular.ttf").as_slice(),
        //         1024,
        //     )
        //     .unwrap();
        // }
        let game_loop_data = GameLoopData {
            clock: Instant::now(),
            lag: 0.0,
        };
        let ctx = &mut engine.user_interfaces.first_mut().build_ctx();

        fn build_checkbox(ctx: &mut BuildContext, text: &str, on_column: usize) -> Handle<UiNode> {
            CheckBoxBuilder::new(
                WidgetBuilder::new()
                    .on_column(on_column)
                    .with_vertical_alignment(VerticalAlignment::Center)
                    .with_margin(Thickness::uniform(1.0)),
            )
            .with_content(
                TextBuilder::new(
                    WidgetBuilder::new()
                        .on_column(0)
                        .with_vertical_alignment(VerticalAlignment::Center),
                )
                .with_text(text)
                .build(ctx),
            )
            .checked(Some(true))
            .build(ctx)
        }
        let info_checkbox = build_checkbox(ctx, "Info", 0);
        let warning_checkbox = build_checkbox(ctx, "Warning", 1);
        let error_checkbox = build_checkbox(ctx, "Error", 2);
        let engine_checkbox = build_checkbox(ctx, "Engine", 3);
        let game_checkbox = build_checkbox(ctx, "Game", 4);
        let log_stack_panel = StackPanelBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::uniform(1.0))
                .with_vertical_alignment(VerticalAlignment::Top)
                .with_horizontal_alignment(HorizontalAlignment::Stretch),
        )
        .with_orientation(Orientation::Vertical)
        .build(ctx);
        // vertical stack panel with horizontal checkboxes on top and a scroll view below
        let root_grid = StackPanelBuilder::new(
            WidgetBuilder::new()
                .with_child(
                    // horizontal stack panel
                    StackPanelBuilder::new(
                        WidgetBuilder::new()
                            .with_horizontal_alignment(HorizontalAlignment::Left)
                            .on_row(0)
                            .with_child(info_checkbox)
                            .with_child(warning_checkbox)
                            .with_child(error_checkbox)
                            .with_child(engine_checkbox)
                            .with_child(game_checkbox),
                    )
                    .build(ctx),
                )
                .with_child(
                    ScrollViewerBuilder::new(
                        WidgetBuilder::new()
                            .with_vertical_alignment(VerticalAlignment::Stretch)
                            .on_row(1)
                            .with_margin(Thickness::uniform(3.0)),
                    )
                    .with_content(log_stack_panel)
                    .with_horizontal_scroll_allowed(true)
                    .with_vertical_scroll_allowed(true)
                    .build(ctx),
                )
                .with_width(600.0)
                .with_height(800.0)
                .with_background(ctx.style.property(Style::BRUSH_DIM_BLUE)),
        )
        .with_orientation(Orientation::Vertical)
        .build(ctx);

        let max_messages = 1000; // hardcoded limit for now
        Self {
            graphics_context_initialized_once: false,
            engine,
            log_message_receiver,
            message_sender,
            message_receiver,
            game_loop_data,
            root_grid,
            info_checkbox,
            warning_checkbox,
            error_checkbox,
            engine_checkbox,
            game_checkbox,
            log_stack_panel,
            messages_from_engine: VecDeque::new(),
            messages_from_game: VecDeque::new(),
            max_messages,
            info_checked: true,
            warning_checked: true,
            error_checked: true,
            engine_checked: true,
            game_checked: true,
        }
    }

    /// The log message UI nodes to be rendered are treated as stateless, meaning that
    /// whenever the log messages change, we remove all message nodes from the stack panel
    /// and recreate them from scratch.
    ///
    /// The performance overhead is negligible compared with the actual game.
    ///
    /// In this way, it is much more maintainable and supports functionalities like folding duplicate log messages
    /// with much simpler implementation.
    fn update_log_message_model(&mut self) {
        let user_interface = self.engine.user_interfaces.first_mut();
        let log_stack_panel_ref = user_interface.node_mut(self.log_stack_panel);
        let children = log_stack_panel_ref.children().to_vec();
        for child in children {
            user_interface.send_message(WidgetMessage::remove(child, MessageDirection::ToWidget));
        }

        // let log_stack_panel_ref = user_interface.node_mut(self.log_stack_panel);
        // assert!(log_stack_panel_ref.children().is_empty());
        //
        let mut visited_messages: FxHashMap<(MessageKind, String), usize> = FxHashMap::default();
        let mut folded_messages_rev: Vec<(MessageKind, String)> = Vec::new();
        for (checked, messages) in [
            (self.game_checked, &self.messages_from_game),
            (self.engine_checked, &self.messages_from_engine),
        ] {
            if checked {
                for message in messages.iter().rev() {
                    if (message.kind == MessageKind::Information && !self.info_checked)
                        || (message.kind == MessageKind::Warning && !self.warning_checked)
                        || (message.kind == MessageKind::Error && !self.error_checked)
                    {
                        continue;
                    }
                    let key = (message.kind, message.content.clone());
                    if let Some(count) = visited_messages.get_mut(&key) {
                        *count += 1;
                    } else {
                        visited_messages.insert(key.clone(), 1);
                        folded_messages_rev.push(key);
                    }
                }
            }
        }
        // create the actual text UI nodes

        for (index, message) in folded_messages_rev.into_iter().rev().enumerate() {
            let ctx = &mut user_interface.build_ctx();
            let count = visited_messages[&message];
            let text = if count > 1 {
                format!("{} (x{})", message.1, count)
            } else {
                message.1.clone()
            };
            // This is copied from the LogPanel.
            let item = BorderBuilder::new(
                WidgetBuilder::new()
                    .with_background(if index % 2 == 0 {
                        ctx.style.property(Style::BRUSH_LIGHT)
                    } else {
                        ctx.style.property(Style::BRUSH_DARK)
                    })
                    .with_child(
                        TextBuilder::new(
                            WidgetBuilder::new()
                                // .with_context_menu(self.context_menu.menu.clone())
                                .with_margin(Thickness::uniform(2.0))
                                .with_foreground(match message.0 {
                                    MessageKind::Information => {
                                        ctx.style.property(Style::BRUSH_INFORMATION)
                                    }
                                    MessageKind::Warning => {
                                        ctx.style.property(Style::BRUSH_WARNING)
                                    }
                                    MessageKind::Error => ctx.style.property(Style::BRUSH_ERROR),
                                }),
                        )
                        .with_vertical_text_alignment(VerticalAlignment::Center)
                        .with_text(text)
                        .build(ctx),
                    ),
            )
            .build(ctx);
            user_interface.send_message(WidgetMessage::link(
                item,
                MessageDirection::ToWidget,
                self.log_stack_panel,
            ))
        }
    }

    /// The update function
    pub fn update(&mut self, event_loop: &ActiveEventLoop) {
        if let GraphicsContext::Initialized(ctx) = &self.engine.graphics_context {
            ctx.make_context_current().unwrap();
        }
        let elapsed = self.game_loop_data.clock.elapsed().as_secs_f32();
        self.game_loop_data.clock = Instant::now();
        self.game_loop_data.lag += elapsed;

        let mut received_anything = false;
        // receive messages from the log
        while let Ok(mut log_message) = self.log_message_receiver.try_recv() {
            received_anything = true;
            if log_message.content.contains("[__GAME__]") {
                log_message.content = log_message.content.replace("[__GAME__]", "");
                self.messages_from_game.push_back(log_message);
                if self.messages_from_game.len() > self.max_messages {
                    self.messages_from_game.pop_front();
                }
            } else {
                self.messages_from_engine.push_back(log_message);
                if self.messages_from_engine.len() > self.max_messages {
                    self.messages_from_engine.pop_front();
                }
            }
        }
        // handle ui messages
        let message_model_requires_update = self.poll_ui_messages();
        if received_anything || message_model_requires_update {
            println!("Log message model updated");
            self.update_log_message_model();
        }

        while self.game_loop_data.lag >= FIXED_TIMESTEP {
            self.game_loop_data.lag -= FIXED_TIMESTEP;
            // let pre_update_switch = GraphUpdateSwitches{

            // }
            self.engine.update(
                FIXED_TIMESTEP,
                ApplicationLoopController::ActiveEventLoop(&event_loop),
                &mut self.game_loop_data.lag,
                Default::default(),
            );
        }

        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                Message::Exit { force: _ } => {
                    panic!("This should be handled directly in WindowEvent::CloseRequested");
                }
                _ => {}
            }
        }
        if let GraphicsContext::Initialized(ctx) = &self.engine.graphics_context {
            ctx.window.request_redraw();
            ctx.make_context_not_current().unwrap();
        }
    }
    /// Calls initialize_graphics_context on the engine.
    pub fn on_resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.engine
            .initialize_graphics_context(event_loop)
            .expect("Unable to initialize graphics context!");
        self.engine
            .graphics_context
            .as_initialized_ref()
            .make_context_not_current()
            .unwrap();
    }

    pub fn on_suspended(&mut self) {
        self.engine.destroy_graphics_context().unwrap();
    }

    /// Returns `true` if the message model requires update.
    fn poll_ui_messages(&mut self) -> bool {
        let mut message_model_requires_update = false;
        while let Some(message) = self.engine.user_interfaces.first_mut().poll_message() {
            if self.handle_ui_message(&message) {
                message_model_requires_update = true;
            }
        }
        message_model_requires_update
    }

    /// Returns `true` if the message model requires update.
    pub fn handle_ui_message(&mut self, message: &UiMessage) -> bool {
        println!("Received a ui message");
        fn handle_check_changed(
            message: &UiMessage,
            checkbox_message: &CheckBoxMessage,
            checkbox: &Handle<UiNode>,
            checked: &mut bool,
        ) {
            if message.destination() == *checkbox
                && message.direction() == MessageDirection::FromWidget
            {
                if let CheckBoxMessage::Check(check) = checkbox_message {
                    *checked = check.map_or(false, |c| c);
                    println!("Checkbox changed, now: {}", *checked);
                }
            }
        }
        let mut message_model_requires_update = false;
        if let Some(checkbox_message) = message.data::<CheckBoxMessage>() {
            handle_check_changed(
                message,
                checkbox_message,
                &self.info_checkbox,
                &mut self.info_checked,
            );
            handle_check_changed(
                message,
                checkbox_message,
                &self.warning_checkbox,
                &mut self.warning_checked,
            );
            handle_check_changed(
                message,
                checkbox_message,
                &self.error_checkbox,
                &mut self.error_checked,
            );
            handle_check_changed(
                message,
                checkbox_message,
                &self.engine_checkbox,
                &mut self.engine_checked,
            );
            handle_check_changed(
                message,
                checkbox_message,
                &self.game_checkbox,
                &mut self.game_checked,
            );
            // If we receive a CheckBoxMessage, it means one of the checkboxes changed state.
            message_model_requires_update = true;
        }
        message_model_requires_update
    }
}
