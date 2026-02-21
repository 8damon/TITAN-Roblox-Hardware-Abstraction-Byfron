use windows::core::Interface;
use windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::{
        ToastActivatedEventArgs, ToastDismissedEventArgs, ToastFailedEventArgs, ToastNotification,
        ToastNotificationManager,
    },
    core::{HSTRING, IInspectable},
};

use tracing::{info, warn};

pub fn ask_user_to_spoof() -> bool {
    match show_toast() {
        Ok(v) => v,
        Err(e) => {
            warn!("Toast notification failed: {e}");
            false
        }
    }
}

fn show_toast() -> anyhow::Result<bool> {
    info!("Displaying spoof notification");

    let xml = r#"
<toast launch="action=spoof">
    <visual>
        <binding template="ToastGeneric">
            <text>TITAN Spoofer</text>
            <text>Roblox has closed. Spoof hardware identifiers?</text>
        </binding>
    </visual>
    <actions>
        <action content="Spoof Now" arguments="spoof" activationType="foreground"/>
        <action content="Ignore" arguments="ignore" activationType="foreground"/>
    </actions>
</toast>
"#;

    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;

    let toast = ToastNotification::CreateToastNotification(&doc)?;

    let (tx, rx) = std::sync::mpsc::channel::<bool>();

    //
    // Activated
    //

    let tx_activated = tx.clone();

    toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
        move |_sender, args| {
            // args is Ref<'_, IInspectable> but deref yields Option<IInspectable> in your bindings
            let inspectable: &IInspectable = match &*args {
                Some(v) => v,
                None => {
                    tx_activated.send(false).ok();
                    return Ok(());
                }
            };

            let activation: ToastActivatedEventArgs = inspectable.cast()?;
            let arg = activation.Arguments()?;
            tx_activated.send(arg == "spoof").ok();
            Ok(())
        },
    ))?;

    //
    // Dismissed
    //

    let tx_dismissed = tx.clone();

    toast.Dismissed(&TypedEventHandler::<
        ToastNotification,
        ToastDismissedEventArgs,
    >::new(move |_sender, _args| {
        tx_dismissed.send(false).ok();
        Ok(())
    }))?;

    //
    // Failed
    //

    toast.Failed(
        &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(
            move |_sender, _args| {
                tx.send(false).ok();
                Ok(())
            },
        ),
    )?;

    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("TITAN.Spoofer"))?;

    notifier.Show(&toast)?;

    Ok(rx.recv().unwrap_or(false))
}
