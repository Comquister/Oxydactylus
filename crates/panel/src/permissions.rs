//! Permission constants for the Oxydactylus panel subuser system.

// Control
pub const CONTROL_CONSOLE: &str = "control.console";
pub const CONTROL_START:   &str = "control.start";
pub const CONTROL_STOP:    &str = "control.stop";
pub const CONTROL_RESTART: &str = "control.restart";

// Users (subuser management)
pub const USER_CREATE: &str = "user.create";
pub const USER_READ:   &str = "user.read";
pub const USER_UPDATE: &str = "user.update";
pub const USER_DELETE: &str = "user.delete";

// Files
pub const FILE_CREATE:       &str = "file.create";
pub const FILE_READ:         &str = "file.read";
pub const FILE_READ_CONTENT: &str = "file.read-content";
pub const FILE_UPDATE:       &str = "file.update";
pub const FILE_DELETE:       &str = "file.delete";
pub const FILE_ARCHIVE:      &str = "file.archive";
pub const FILE_SFTP:         &str = "file.sftp";

// Backups
pub const BACKUP_CREATE:   &str = "backup.create";
pub const BACKUP_READ:     &str = "backup.read";
pub const BACKUP_DELETE:   &str = "backup.delete";
pub const BACKUP_DOWNLOAD: &str = "backup.download";
pub const BACKUP_RESTORE:  &str = "backup.restore";

// Network
pub const NETWORK_READ:   &str = "network.read";
pub const NETWORK_CREATE: &str = "network.create";
pub const NETWORK_UPDATE: &str = "network.update";
pub const NETWORK_DELETE: &str = "network.delete";

// Startup
pub const STARTUP_READ:         &str = "startup.read";
pub const STARTUP_UPDATE:       &str = "startup.update";
pub const STARTUP_DOCKER_IMAGE: &str = "startup.docker-image";

// Databases
pub const DATABASE_CREATE:        &str = "database.create";
pub const DATABASE_READ:          &str = "database.read";
pub const DATABASE_UPDATE:        &str = "database.update";
pub const DATABASE_DELETE:        &str = "database.delete";
pub const DATABASE_VIEW_PASSWORD: &str = "database.view-password";

// Schedules
pub const SCHEDULE_CREATE: &str = "schedule.create";
pub const SCHEDULE_READ:   &str = "schedule.read";
pub const SCHEDULE_UPDATE: &str = "schedule.update";
pub const SCHEDULE_DELETE: &str = "schedule.delete";

// Importer
pub const IMPORTER_ACCESS: &str = "importer.access";

// Settings
pub const SETTINGS_RENAME:     &str = "settings.rename";
pub const SETTINGS_REINSTALL:  &str = "settings.reinstall";
pub const SETTINGS_CHANGE_EGG: &str = "settings.change-egg";

// Activity
pub const ACTIVITY_READ: &str = "activity.read";

/// INVARIANT: every public const in this module must appear exactly once in this slice.
pub const ALL_PERMISSIONS: &[(&str, &[&str])] = &[
    ("control",  &[CONTROL_CONSOLE, CONTROL_START, CONTROL_STOP, CONTROL_RESTART]),
    ("user",     &[USER_CREATE, USER_READ, USER_UPDATE, USER_DELETE]),
    ("file",     &[FILE_CREATE, FILE_READ, FILE_READ_CONTENT, FILE_UPDATE, FILE_DELETE, FILE_ARCHIVE, FILE_SFTP]),
    ("backup",   &[BACKUP_CREATE, BACKUP_READ, BACKUP_DELETE, BACKUP_DOWNLOAD, BACKUP_RESTORE]),
    ("network",  &[NETWORK_READ, NETWORK_CREATE, NETWORK_UPDATE, NETWORK_DELETE]),
    ("startup",  &[STARTUP_READ, STARTUP_UPDATE, STARTUP_DOCKER_IMAGE]),
    ("database", &[DATABASE_CREATE, DATABASE_READ, DATABASE_UPDATE, DATABASE_DELETE, DATABASE_VIEW_PASSWORD]),
    ("schedule", &[SCHEDULE_CREATE, SCHEDULE_READ, SCHEDULE_UPDATE, SCHEDULE_DELETE]),
    ("importer", &[IMPORTER_ACCESS]),
    ("settings", &[SETTINGS_RENAME, SETTINGS_REINSTALL, SETTINGS_CHANGE_EGG]),
    ("activity", &[ACTIVITY_READ]),
];

/// Retorna true se a string é uma permissão válida conhecida.
pub fn is_valid_permission(p: &str) -> bool {
    ALL_PERMISSIONS.iter().any(|(_, perms)| perms.contains(&p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_permissions_non_empty() {
        assert!(!ALL_PERMISSIONS.is_empty());
        for (group, perms) in ALL_PERMISSIONS {
            assert!(!perms.is_empty(), "group {} has no permissions", group);
        }
    }

    #[test]
    fn control_start_is_valid() {
        assert!(is_valid_permission(CONTROL_START));
    }

    #[test]
    fn unknown_permission_is_invalid() {
        assert!(!is_valid_permission("hacker.pwn"));
    }

    #[test]
    fn permission_strings_use_dot_convention() {
        for (_, perms) in ALL_PERMISSIONS {
            for p in *perms {
                assert!(p.contains('.'), "permission '{}' missing dot separator", p);
                assert_eq!(*p, p.to_lowercase(), "permission '{}' not lowercase", p);
            }
        }
    }
}
