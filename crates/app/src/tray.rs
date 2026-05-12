//! Системный трей: значок + меню «Открыть / Запустить всё / Выход».

use std::sync::Arc;

use slint::{ComponentHandle, Weak};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

use application::Scheduler;

use crate::AppWindow;

pub struct TrayManager {
    _tray: TrayIcon, // держим живым: drop = иконка исчезает
    open_id: tray_icon::menu::MenuId,
    run_all_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    window: Weak<AppWindow>,
    scheduler: Arc<Scheduler>,
}

impl TrayManager {
    pub fn new(window: Weak<AppWindow>, scheduler: Arc<Scheduler>) -> anyhow::Result<Self> {
        let menu = Menu::new();

        let open_item = MenuItem::new("Открыть", true, None);
        let run_all_item = MenuItem::new("Запустить все сейчас", true, None);
        let sep = PredefinedMenuItem::separator();
        let quit_item = MenuItem::new("Выход", true, None);

        menu.append(&open_item)?;
        menu.append(&run_all_item)?;
        menu.append(&sep)?;
        menu.append(&quit_item)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Backuper")
            .with_icon(make_icon())
            .build()?;

        Ok(Self {
            _tray: tray,
            open_id: open_item.id().clone(),
            run_all_id: run_all_item.id().clone(),
            quit_id: quit_item.id().clone(),
            window,
            scheduler,
        })
    }

    /// Опросить очередь событий меню.
    /// Вызывается из Slint-таймера на главном потоке каждые ~100 мс.
    pub fn poll(&self) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.open_id {
                if let Some(w) = self.window.upgrade() {
                    w.show().ok();
                }
            } else if event.id == self.run_all_id {
                let sched = self.scheduler.clone();
                tokio::spawn(async move {
                    sched.enqueue_all_enabled().await;
                });
            } else if event.id == self.quit_id {
                slint::quit_event_loop().ok();
            }
        }
    }
}

/// Программная 16×16 иконка (синий квадрат).
fn make_icon() -> tray_icon::Icon {
    const SIZE: u32 = 16;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for chunk in rgba.chunks_mut(4) {
        chunk[0] = 29;
        chunk[1] = 113;
        chunk[2] = 206;
        chunk[3] = 255;
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("tray icon")
}
