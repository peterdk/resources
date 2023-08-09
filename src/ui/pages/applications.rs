use std::cell::Ref;

use adw::ResponseAppearance;
use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, BoxedAnyObject, Object, Sender};
use gtk::{gio, CustomSorter, FilterChange, Ordering, SortType};
use gtk_macros::send;

use log::error;

use crate::config::PROFILE;
use crate::i18n::i18n;
use crate::ui::dialogs::app_dialog::ResAppDialog;
use crate::ui::widgets::application_name_cell::ResApplicationNameCell;
use crate::ui::window::{self, Action, MainWindow};
use crate::utils::processes::{AppItem, AppsContext, ProcessAction};
use crate::utils::units::{to_largest_unit, Base};

mod imp {
    use std::cell::RefCell;

    use crate::ui::window::Action;

    use super::*;

    use gtk::{glib::Sender, CompositeTemplate};
    use once_cell::sync::OnceCell;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/me/nalux/Resources/ui/pages/applications.ui")]
    pub struct ResApplications {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub search_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub applications_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub search_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub information_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub end_application_button: TemplateChild<adw::SplitButton>,

        pub store: RefCell<gio::ListStore>,
        pub selection_model: RefCell<gtk::SingleSelection>,
        pub filter_model: RefCell<gtk::FilterListModel>,
        pub sort_model: RefCell<gtk::SortListModel>,
        pub column_view: RefCell<gtk::ColumnView>,
        pub open_dialog: RefCell<Option<(String, ResAppDialog)>>,

        pub sender: OnceCell<Sender<Action>>,
    }

    impl Default for ResApplications {
        fn default() -> Self {
            Self {
                toast_overlay: Default::default(),
                search_revealer: Default::default(),
                search_entry: Default::default(),
                search_button: Default::default(),
                information_button: Default::default(),
                store: gio::ListStore::new::<BoxedAnyObject>().into(),
                selection_model: Default::default(),
                filter_model: Default::default(),
                sort_model: Default::default(),
                column_view: Default::default(),
                open_dialog: Default::default(),
                sender: Default::default(),
                applications_scrolled_window: Default::default(),
                end_application_button: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResApplications {
        const NAME: &'static str = "ResApplications";
        type Type = super::ResApplications;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action(
                "applications.kill-application",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_item() {
                        res_applications.execute_process_action_dialog(app, ProcessAction::KILL);
                    }
                },
            );

            klass.install_action(
                "applications.halt-application",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_item() {
                        res_applications.execute_process_action_dialog(app, ProcessAction::STOP);
                    }
                },
            );

            klass.install_action(
                "applications.continue-application",
                None,
                move |res_applications, _, _| {
                    if let Some(app) = res_applications.get_selected_app_item() {
                        res_applications.execute_process_action_dialog(app, ProcessAction::CONT);
                    }
                },
            );

            Self::bind_template(klass);
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResApplications {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
        }
    }

    impl WidgetImpl for ResApplications {}
    impl BinImpl for ResApplications {}
}

glib::wrapper! {
    pub struct ResApplications(ObjectSubclass<imp::ResApplications>)
        @extends gtk::Widget, adw::Bin;
}

impl ResApplications {
    pub fn new() -> Self {
        glib::Object::new::<Self>()
    }

    pub fn init(&self, sender: Sender<Action>) {
        let imp = self.imp();
        imp.sender.set(sender).unwrap();

        self.setup_widgets();
        self.setup_signals();
    }

    pub fn setup_widgets(&self) {
        let imp = self.imp();

        let column_view = gtk::ColumnView::new(None::<gtk::SingleSelection>);
        let store = imp.store.borrow_mut();
        let filter_model = gtk::FilterListModel::new(
            Some(store.clone()),
            Some(gtk::CustomFilter::new(
                clone!(@strong self as this => move |obj| this.search_filter(obj)),
            )),
        );
        let sort_model = gtk::SortListModel::new(Some(filter_model.clone()), column_view.sorter());
        let selection_model = gtk::SingleSelection::new(Some(sort_model.clone()));
        column_view.set_model(Some(&selection_model));
        selection_model.set_can_unselect(true);
        selection_model.set_autoselect(false);

        *imp.selection_model.borrow_mut() = selection_model;
        *imp.sort_model.borrow_mut() = sort_model;
        *imp.filter_model.borrow_mut() = filter_model;

        let name_col_factory = gtk::SignalListItemFactory::new();
        let name_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Application")), Some(name_col_factory.clone()));
        name_col.set_resizable(true);
        name_col.set_expand(true);
        name_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = ResApplicationNameCell::new();
            item.set_child(Some(&row));
        });
        name_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<ResApplicationNameCell>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<AppItem> = entry.borrow();
            child.set_name(&r.display_name);
            child.set_icon(Some(&r.icon));
        });
        let name_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<AppItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<AppItem>();
            item_a.display_name.cmp(&item_b.display_name).into()
        });
        name_col.set_sorter(Some(&name_col_sorter));

        let memory_col_factory = gtk::SignalListItemFactory::new();
        let memory_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Memory")), Some(memory_col_factory.clone()));
        memory_col.set_resizable(true);
        memory_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Inscription::new(None);
            item.set_child(Some(&row));
        });
        memory_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<gtk::Inscription>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<AppItem> = entry.borrow();
            let (number, prefix) = to_largest_unit(r.memory_usage as f64, &Base::Decimal);
            child.set_text(Some(&format!("{number:.1} {prefix}B")));
        });
        let memory_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<AppItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<AppItem>();
            item_a.memory_usage.cmp(&item_b.memory_usage).into()
        });
        memory_col.set_sorter(Some(&memory_col_sorter));

        let cpu_col_factory = gtk::SignalListItemFactory::new();
        let cpu_col =
            gtk::ColumnViewColumn::new(Some(&i18n("Processor")), Some(cpu_col_factory.clone()));
        cpu_col.set_resizable(true);
        cpu_col_factory.connect_setup(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Inscription::new(None);
            item.set_child(Some(&row));
        });
        cpu_col_factory.connect_bind(move |_factory, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let child = item
                .child()
                .unwrap()
                .downcast::<gtk::Inscription>()
                .unwrap();
            let entry = item.item().unwrap().downcast::<BoxedAnyObject>().unwrap();
            let r: Ref<AppItem> = entry.borrow();
            child.set_text(Some(&format!("{:.1} %", r.cpu_time_ratio * 100.0)));
        });
        let cpu_col_sorter = CustomSorter::new(move |a, b| {
            let item_a = a
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<AppItem>();
            let item_b = b
                .downcast_ref::<BoxedAnyObject>()
                .unwrap()
                .borrow::<AppItem>();
            // we have to do this because f32s do not implement `Ord`
            if item_a.cpu_time_ratio > item_b.cpu_time_ratio {
                Ordering::Larger
            } else if item_a.cpu_time_ratio < item_b.cpu_time_ratio {
                Ordering::Smaller
            } else {
                Ordering::Equal
            }
        });
        cpu_col.set_sorter(Some(&cpu_col_sorter));

        column_view.append_column(&name_col);
        column_view.append_column(&memory_col);
        column_view.append_column(&cpu_col);
        column_view.sort_by_column(Some(&name_col), SortType::Ascending);
        column_view.set_enable_rubberband(true);
        imp.applications_scrolled_window
            .set_child(Some(&column_view));
        *imp.column_view.borrow_mut() = column_view;
    }

    pub fn setup_signals(&self) {
        let imp = self.imp();

        imp.selection_model.borrow().connect_selection_changed(
            clone!(@strong self as this => move |model, _, _| {
                let imp = this.imp();
                let is_system_processes = model.selected_item().map_or(false, |object| {
                    object
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<AppItem>()
                    .clone()
                    .id
                    .is_none()
                });
                imp.information_button.set_sensitive(model.selected() != u32::MAX);
                imp.end_application_button.set_sensitive(model.selected() != u32::MAX && !is_system_processes);
            }),
        );

        imp.search_button
            .connect_toggled(clone!(@strong self as this => move |button| {
                let imp = this.imp();
                imp.search_revealer.set_reveal_child(button.is_active());
                if let Some(filter) = imp.filter_model.borrow().filter() {
                    filter.changed(FilterChange::Different);
                }
                if button.is_active() {
                    imp.search_entry.grab_focus();
                }
            }));

        imp.search_entry
            .connect_search_changed(clone!(@strong self as this => move |_| {
                let imp = this.imp();
                if let Some(filter) = imp.filter_model.borrow().filter() {
                    filter.changed(FilterChange::Different);
                }
            }));

        imp.information_button
            .connect_clicked(clone!(@strong self as this => move |_| {
                let imp = this.imp();
                let selection_option = imp.selection_model.borrow()
                .selected_item()
                .map(|object| {
                    object
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<AppItem>()
                    .clone()
                });
                if let Some(selection) = selection_option {
                    let app_dialog = ResAppDialog::new();
                    app_dialog.init(&selection);
                    app_dialog.show();
                    *imp.open_dialog.borrow_mut() = Some((selection.id.unwrap_or_default(), app_dialog));
                }
            }));

        imp.end_application_button
            .connect_clicked(clone!(@strong self as this => move |_| {
                if let Some(app) = this.get_selected_app_item() {
                    this.execute_process_action_dialog(app, ProcessAction::TERM);
                }
            }));
    }

    fn search_filter(&self, obj: &Object) -> bool {
        let imp = self.imp();
        let item = obj
            .downcast_ref::<BoxedAnyObject>()
            .unwrap()
            .borrow::<AppItem>()
            .clone();
        let search_string = imp.search_entry.text().to_string().to_lowercase();
        !imp.search_revealer.reveals_child()
            || item.display_name.to_lowercase().contains(&search_string)
            || item
                .description
                .unwrap_or_default()
                .to_lowercase()
                .contains(&search_string)
    }

    fn get_selected_app_item(&self) -> Option<AppItem> {
        self.imp()
            .selection_model
            .borrow()
            .selected_item()
            .map(|object| {
                object
                    .downcast::<BoxedAnyObject>()
                    .unwrap()
                    .borrow::<AppItem>()
                    .clone()
            })
    }

    pub fn refresh_apps_list(&self, apps: &AppsContext) {
        let imp = self.imp();
        let selection = imp.selection_model.borrow();

        // if we reuse the old ListStore, for some reason setting the
        // vadjustment later just doesn't work most of the time.
        // so we just make a new one every refresh instead :')
        // TODO: make this less hacky
        let new_store = gio::ListStore::new::<BoxedAnyObject>();

        // this might be very hacky, but remember the ID of the currently
        // selected item, clear the list model and repopulate it with the
        // refreshed apps and stats, then reselect the remembered app.
        // TODO: make this even less hacky
        let selected_item = self.get_selected_app_item().map(|app_item| app_item.id);
        apps.app_items()
            .iter()
            .map(|app_item| {
                if let Some((id, dialog)) = &*imp.open_dialog.borrow() && app_item.id.clone().unwrap_or_default().as_str() == id.as_str() {
                    dialog.set_cpu_usage(app_item.cpu_time_ratio);
                    dialog.set_memory_usage(app_item.memory_usage);
                    dialog.set_processes_amount(app_item.processes_amount);
                }
                BoxedAnyObject::new(app_item.clone())
            })
            .for_each(|item_box| new_store.append(&item_box));

        imp.filter_model.borrow().set_model(Some(&new_store));
        *imp.store.borrow_mut() = new_store;

        // find the (potentially) new index of the process that was selected
        // before the refresh and set our selection to that index
        if let Some(selected_item) = selected_item {
            let new_index = selection
                .iter::<glib::Object>()
                .position(|object| {
                    object
                        .unwrap()
                        .downcast::<BoxedAnyObject>()
                        .unwrap()
                        .borrow::<AppItem>()
                        .id
                        == selected_item
                })
                .map(|index| index as u32);
            if let Some(index) = new_index && index != u32::MAX {
                selection.set_selected(index);
            }
        }
    }

    pub fn execute_process_action_dialog(&self, app: AppItem, action: ProcessAction) {
        let imp = self.imp();

        // Nothing too bad can happen on Continue so dont show the dialog
        if action == ProcessAction::CONT {
            send!(
                imp.sender.get().unwrap(),
                Action::ManipulateApp(action, app.id.unwrap(), self.imp().toast_overlay.get())
            );
            return;
        }

        // Confirmation dialog & warning
        let dialog = adw::MessageDialog::builder()
            .transient_for(&MainWindow::default())
            .modal(true)
            .heading(window::get_action_name(action, &[&app.display_name]))
            .body(window::get_app_action_warning(action))
            .build();

        dialog.add_response("yes", &window::get_app_action_description(action));
        dialog.set_response_appearance("yes", ResponseAppearance::Destructive);

        dialog.add_response("no", &i18n("Cancel"));
        dialog.set_default_response(Some("no"));
        dialog.set_close_response("no");

        // Called when "yes" or "no" were clicked
        dialog.connect_response(
            None,
            clone!(@strong self as this, @strong app => move |_, response| {
                if response == "yes" {
                    let imp = this.imp();
                    send!(
                        imp.sender.get().unwrap(),
                        Action::ManipulateApp(action, app.id.clone().unwrap(), imp.toast_overlay.get())
                    );
                }
            }),
        );

        dialog.show();
    }
}
