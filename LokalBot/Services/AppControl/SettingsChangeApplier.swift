import Foundation

/// Applies checked plan items to `AppSettings` through the real decoder, so a
/// value the app would reject or adjust is reported instead of silently stored
/// as something else, then applies the couplings Settings enforces.
enum SettingsChangeApplier {
    static func apply(
        _ items: [SettingsChangePlan.Item],
        to settings: AppSettings,
        now: Date = Date(),
        calendar: Calendar = .current
    ) throws -> AppSettings {
        var object = AppControlHandler.encoded(settings)
        for item in items {
            // Assigning nil removes an optional key, which restores its default.
            object[item.spec.key] = item.stored
        }
        let decoded: AppSettings
        do {
            decoded = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(object))
        } catch {
            throw AppControlError.invalidRequest("LokalBot could not read the changed settings: \(error.localizedDescription)")
        }
        let result = AppControlHandler.encoded(decoded)
        for item in items where decoded.corruptedSettingsKeys.contains(item.spec.key) || result[item.spec.key] != item.stored {
            throw AppControlError.invalidValue(item.spec.key, "isn't a value LokalBot accepts")
        }
        var updated = decoded
        updated.corruptedSettingsKeys = settings.corruptedSettingsKeys
        return couple(updated, from: settings, now: now, calendar: calendar)
    }

    /// The rules the Settings controls apply alongside a change, so a change
    /// from the terminal leaves the same settings a click would.
    static func couple(_ new: AppSettings, from old: AppSettings, now: Date, calendar: Calendar) -> AppSettings {
        var settings = new
        if old.trackingEnabled, !settings.trackingEnabled {
            settings.screenContextCaptureMode = .activityOnly
            settings.screenshotsEnabled = false
        }
        if settings.screenContextCaptureMode != old.screenContextCaptureMode {
            settings.screenshotsEnabled = settings.screenContextCaptureMode.capturesPixels
            if settings.screenContextCaptureMode.capturesText { settings.trackingEnabled = true }
        }
        if settings.dreamingEnabled != old.dreamingEnabled {
            settings.dreamingFirstEligibleDayKey = AppState.dreamingFirstEligibleDayKey(
                enabled: settings.dreamingEnabled, now: now, calendar: calendar)
        }
        return settings
    }
}
