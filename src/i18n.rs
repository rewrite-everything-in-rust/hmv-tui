//! UI translations (English / Spanish). `Lang` is stored in the config
//! file and switched from the dashboard with `l`. Every user-visible
//! string lives here as a `Key` so the two languages stay in sync.

/// UI language. Persisted as `"en"` / `"es"` in `~/.hmv/config.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    /// The other language (toggle target).
    pub fn other(self) -> Lang {
        match self {
            Lang::En => Lang::Es,
            Lang::Es => Lang::En,
        }
    }

    /// Short label shown in the footer hint.
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Es => "ES",
        }
    }

    /// Parses the persisted config value ("en"/"es"); unknown → En.
    pub fn from_config(value: &str) -> Lang {
        match value.trim().to_lowercase().as_str() {
            "es" => Lang::Es,
            _ => Lang::En,
        }
    }

    /// Config-file spelling of this language.
    pub fn to_config(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    /// Looks up the translation for `key` in this language.
    pub fn t(self, key: Key) -> &'static str {
        match self {
            Lang::En => en(key),
            Lang::Es => es(key),
        }
    }
}

/// Every translatable string in the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // Header / tabs
    HeaderDashboard,
    TabStats,
    TabWriteups,
    TabPending,
    TabMachines,
    TabReleases,
    TabSubmissions,

    // Submissions
    ColLevel,
    ColUser,
    SubmitTags,
    SubmitFormTitle,
    SubmitName,
    SubmitUrl,
    SubmitUserFlag,
    SubmitRootFlag,
    SubmitWriteup,
    SubmitNotes,
    SubmitFormNotice,
    SubmitFormHint,
    SubmitLevel,
    SubmitOk,
    SubmitFailed,
    SubmitUnknown,
    SubmitRequiredMissing,
    SubmitInvalidLevel,
    RulesTitle,
    RulesUnavailable,
    FetchingForm,
    FooterSubmitVm,
    FooterRules,
    MySubmissionsTitle,
    QueueTitle,
    StatusSubmitOnlySubmissions,

    // Stats tab blocks
    StatsIdentity,
    StatsRank,
    StatsTitle,
    StatsCountry,
    StatsLoved,
    StatsAchievements,
    StatsPoints,
    StatsTotalRoots,
    StatsTotalUsers,
    StatsFirstRoots,
    StatsFirstUsers,
    StatsChallenges,
    StatsWriteups,
    StatsTrophies,
    StatsProgress,

    // Filter block
    FilterLabel,
    PwnedHiddenIndicator,

    // Footer fragments
    FooterJkMove,
    FooterFilter,
    FooterEnterOpen,
    FooterWriteup,
    FooterSizeSort,
    FooterHidePwned,
    FooterFlag,
    FooterDownload,
    FooterAccount,
    FooterRefresh,
    FooterQuit,
    FooterTabSwitch,
    FooterLangHint,
    FooterFilterMode,
    CompactUser,
    CompactRoot,
    CompactMachineNotFound,
    CompactUnknown,

    // Popup titles & prompts
    PopupSubmitFlags,
    FlagResultsTitle,
    WriteupResultsTitle,
    PopupSubmitWriteup,
    PopupDownload,
    PopupConfigure,
    PopupUserFlag,
    PopupRootFlag,
    PopupWriteupUrl,
    PopupSaveTo,
    PopupUsername,
    PopupPassword,
    PopupHintFlag,
    PopupHintSend,
    PopupHintDownload,
    PopupHintConfig,
    PopupDirectories,
    PopupAccountTitle,
    PopupLoggedInAs,
    PopupAccountHint,
    PopupPwnedTitle,
    PopupPwnedLine1,
    PopupPwnedLine2,
    PopupPwnedClose,
    NoticeFirstRun,
    NoticeLoginFailed,
    NoticeSwitch,
    NoticeLoggedOut,
    NoticeOneFlagRemains,
    ReportHintRefresh,
    ReportHintClose,
    DirectoriesMore,
    FetchingLoading,
    FetchingRefreshing,
    FetchingLogout,
    FetchingFlag,
    FetchingWriteup,
    FetchingWriteupsList,

    // Writeups popup
    WriteupsPopupTitle,
    WriteupsPopupHint,
    ColDate,
    ColAuthor,
    ColLanguage,
    ColFormat,
    ColLink,

    // Table headers
    ColVm,
    ColDifficulty,
    ColCreator,
    ColSize,
    ColOs,
    ColCompat,
    ColStatus,
    NoCompat,

    // Downloads overlay
    DownloadsTitle,
    DownloadsEmpty,
    DownloadsResolving,
    DownloadsHint,

    // Statuses — generic
    StatusCancelled,
    StatusBusy,
    StatusDataRefreshed,
    StatusFetchFailed,
    StatusActionFailed,
    StatusNothingSelected,
    StatusNothingToInspect,
    StatusOpenedBrowser,
    StatusXdgOpenFailed,
    StatusConnecting,
    StatusConnectedAs,
    StatusConfigFailed,
    StatusLoggedOut,
    StatusLogoutFailed,
    StatusCredentialsRequired,
    StatusEmptyInput,
    StatusAlreadyPwned,
    StatusCancelDownload,

    // Statuses — machines tab
    StatusHidePwnedOnly,
    StatusPwnedHidden,
    StatusPwnedShown,
    StatusSortOnly,
    StatusFlagOnlyMachines,
    StatusDownloadOnlyMachines,
    StatusUploadOnlyPending,
    StatusWriteupsTabs,

    // Statuses — popups & downloads
    StatusLangSwitched,
    StatusDownloadStarted,
    StatusDownloadQueued,
    StatusDownloadFailed,
    StatusQueuedFlag,
    StatusQueuedFlags,
    StatusQueuedWriteup,
    StatusQueuedDownload,
    StatusNoWriteups,

    // Report entries
    ReportFlagAccepted,
    ReportFlagRejected,
    ReportMachineNotFound,
    ReportUnknownResponse,
    ReportYouHacked,
    ReportWriteupAccepted,
    ReportWriteupRepeated,
    ReportWriteupRejected,

    // Quit warning
    QuitWarnDownloads,
}

fn en(key: Key) -> &'static str {
    use Key::*;
    match key {
        // Header / tabs
        HeaderDashboard => " HackMyVM dashboard",
        TabStats => "Stats",
        TabWriteups => "Writeups",
        TabPending => "Pending",
        TabMachines => "Machines",
        TabReleases => "Releases",
        TabSubmissions => "Submissions",
        ColLevel => "Level",
        ColUser => "User",
        SubmitFormTitle => " Submit your VM ",
        SubmitName => "VM name:",
        SubmitUrl => "Download URL:",
        SubmitUserFlag => "User flag:",
        SubmitRootFlag => "Root flag:",
        SubmitWriteup => "Writeup URL:",
        SubmitTags => "Tags:",
        SubmitNotes => "Notes (optional):",
        SubmitFormNotice => "Read the rules (i) before submitting.",
        SubmitFormHint => "↑↓/Tab field · level cycles on Tab · Enter send · Esc cancel",
        SubmitLevel => "Level:",
        SubmitOk => "[✓] VM submitted — waiting in the queue.",
        SubmitFailed => "[!] Submission rejected: {}",
        SubmitUnknown => "[?] Unknown response: {}",
        SubmitRequiredMissing => "Required fields missing: {}",
        SubmitInvalidLevel => "Level must be easy, medium, or hard.",
        RulesTitle => " Submission rules ",
        RulesUnavailable => "Rules unavailable — try refreshing.",
        FetchingForm => "Loading submission form...",
        FooterSubmitVm => "v submit VM",
        FooterRules => "i rules",
        MySubmissionsTitle => " My submissions ",
        QueueTitle => " Queue ",
        StatusSubmitOnlySubmissions => {
            "Submissions actions are only available on the Submissions tab."
        }

        // Footer fragments
        FooterJkMove => "jk move",
        // Stats blocks
        StatsIdentity => "[ Identity ]",
        StatsRank => "  Rank      : ",
        StatsTitle => "  Title     : ",
        StatsCountry => "  Country   : ",
        StatsLoved => "  Loved     : ",
        StatsAchievements => "[ Achievements ]",
        StatsPoints => "  Points      : ",
        StatsTotalRoots => "  Total Roots : ",
        StatsTotalUsers => "  Total Users : ",
        StatsFirstRoots => "  First Roots : ",
        StatsFirstUsers => "  First Users : ",
        StatsChallenges => "  Challenges  : ",
        StatsWriteups => "  Writeups    : ",
        StatsTrophies => "[ Trophies ] ({})",
        StatsProgress => "[ Progress ]",

        // Filter block
        FilterLabel => " filter: ",
        PwnedHiddenIndicator => " · PWNED hidden (h)",

        CompactUser => "User",
        CompactRoot => "Root",
        CompactMachineNotFound => "machine not found",
        CompactUnknown => "unknown",

        // Footer fragments
        FooterFilter => "/ filter",
        FooterEnterOpen => "Enter open",
        FooterWriteup => "u writeup",
        FooterSizeSort => "s size sort",
        FooterHidePwned => "h hide PWNED",
        FooterFlag => "f flag",
        FooterDownload => "d download",
        FooterAccount => "a account",
        FooterRefresh => "r refresh",
        FooterQuit => "q quit",
        FooterTabSwitch => "Tab switch",
        FooterLangHint => "l language",
        FooterFilterMode => "Enter confirm · Esc clear & exit filter",

        // Popups
        PopupSubmitFlags => " Submit flags — {} ",
        FlagResultsTitle => " Flag results — {} ",
        WriteupResultsTitle => " Writeup results — {} ",
        PopupSubmitWriteup => " Submit writeup — {} ",
        PopupDownload => " Download — {} ",
        PopupConfigure => " Configure HackMyVM ",
        PopupUserFlag => "User flag:",
        PopupRootFlag => "Root flag:",
        PopupWriteupUrl => "Writeup URL:",
        PopupSaveTo => "Save to:",
        PopupUsername => "Username:",
        PopupPassword => "Password:",
        PopupHintFlag => "Enter send both · ↑↓/Tab switch field · Esc cancel",
        PopupHintSend => "Enter send · Esc cancel",
        PopupHintDownload => "Tab complete path · Enter start · Esc cancel",
        PopupHintConfig => "Enter save & connect · ↑↓/Tab switch field · Esc quit",
        PopupDirectories => "  directories:",
        PopupAccountTitle => " Account — {} ",
        PopupLoggedInAs => "Logged in as {}",
        PopupAccountHint => "Enter switch account · l logout · Esc close",
        PopupPwnedTitle => " ✓ Already PWNED — {} ",
        PopupPwnedLine1 => "User & root flags are in.",
        PopupPwnedLine2 => "Resubmission is disabled.",
        PopupPwnedClose => "Enter / Esc close",
        NoticeFirstRun => "First run — enter your HackMyVM account.",
        NoticeLoginFailed => "Login failed — re-enter your HackMyVM credentials.",
        NoticeSwitch => "Switch account — enter the new credentials.",
        NoticeLoggedOut => "Logged out — sign in with your HackMyVM account.",
        NoticeOneFlagRemains => "One flag already submitted — one remains.",
        ReportHintRefresh => "Data will refresh on close · Enter / Esc close",
        ReportHintClose => "Enter / Esc close",
        DirectoriesMore => "  … and {} more",

        // Writeups popup
        WriteupsPopupTitle => " Writeups — {} ",
        WriteupsPopupHint => "Enter open link · jk select · Esc close",
        ColDate => "Date",
        ColAuthor => "Author (Poet)",
        ColLanguage => "Language",
        ColFormat => "Format",
        ColLink => "Link",

        // Table headers
        ColVm => "VM",
        ColDifficulty => "Difficulty",
        ColCreator => "Creator",
        ColSize => "Size",
        ColOs => "OS",
        ColCompat => "Tested",
        ColStatus => "Status",
        NoCompat => "-",

        // Downloads overlay
        DownloadsTitle => " Downloads ",
        DownloadsEmpty => "No downloads yet — press d on a machine.",
        DownloadsResolving => "… {}  resolving MEGA link…",
        DownloadsHint => {
            "o close (downloads keep running) · c cancel latest · q quit warns while active"
        }

        // Statuses — generic
        StatusCancelled => "Cancelled.",
        StatusBusy => "Busy — wait for the current operation.",
        StatusDataRefreshed => "Data refreshed.",
        StatusFetchFailed => "Fetch failed: {}",
        StatusActionFailed => "Action failed: {}",
        StatusNothingSelected => "Nothing selected to act on.",
        StatusNothingToInspect => "Nothing selected to inspect.",
        StatusOpenedBrowser => "Opened in browser: {}",
        StatusXdgOpenFailed => "xdg-open failed: {}",
        StatusConnecting => "Connecting...",
        StatusConnectedAs => "[✓] Connected as {} — loading data...",
        StatusConfigFailed => "Configuration failed: {}",
        StatusLoggedOut => "[✓] Logged out — enter another account or Esc to quit.",
        StatusLogoutFailed => "Logout failed: {}",
        StatusCredentialsRequired => "Username and password are required.",
        StatusEmptyInput => "Cancelled — empty input.",
        StatusAlreadyPwned => "{} is already PWNED — nothing to submit.",
        StatusCancelDownload => "Cancelling {}...",

        // Statuses — machines tab
        StatusHidePwnedOnly => "Hide PWNED is only available on the Machines tab.",
        StatusPwnedHidden => "PWNED machines hidden — press h to show them again.",
        StatusPwnedShown => "PWNED machines shown.",
        StatusSortOnly => "Size sort is only available on the Machines tab.",
        StatusFlagOnlyMachines => "Flag submission is only available on the Machines tab.",
        StatusDownloadOnlyMachines => "Downloads are only available on the Machines tab.",
        StatusUploadOnlyPending => "Writeup submission is only available on the Pending tab.",
        StatusWriteupsTabs => "Writeups are available on the Machines and Pending tabs.",

        // Statuses — popups & downloads
        StatusLangSwitched => "Language switched to {}. · l to toggle again",
        StatusDownloadStarted => "[↓] Download {} started.",
        StatusDownloadQueued => "[↓] {} queued — {} downloads active.",
        StatusDownloadFailed => "Download failed: {}",
        StatusQueuedFlag => "Queued flag for {}...",
        StatusQueuedFlags => "Queued flags for {}...",
        StatusQueuedWriteup => "Queued writeup URL for {}...",
        StatusQueuedDownload => "Queued download for {}...",
        StatusNoWriteups => "No community writeups found for {}.",

        // Report entries
        ReportFlagAccepted => "{}: ✓ ACCEPTED",
        ReportFlagRejected => "{}: ✗ REJECTED",
        ReportMachineNotFound => "Machine '{}' not found",
        ReportUnknownResponse => "Unknown response: {}",
        ReportYouHacked => "[✓] You hacked {}!",
        ReportWriteupAccepted => "Writeup: ✓ ACCEPTED — {}",
        ReportWriteupRepeated => "Writeup: [=] ALREADY SUBMITTED",
        ReportWriteupRejected => "Writeup: ✗ REJECTED — flags missing?",

        // Quit warning
        QuitWarnDownloads => "{} download(s) active — press q again to abort: {}",
        FetchingLoading => "Loading data...",
        FetchingRefreshing => "Refreshing data...",
        FetchingLogout => "Logging out...",
        FetchingFlag => "Submitting flag for {}...",
        FetchingWriteup => "Submitting writeup for {}...",
        FetchingWriteupsList => "Loading writeups for {}...",
    }
}

fn es(key: Key) -> &'static str {
    use Key::*;
    match key {
        // Header / tabs
        HeaderDashboard => " HackMyVM panel",
        TabStats => "Estadísticas",
        TabWriteups => "Writeups",
        TabPending => "Pendientes",
        TabMachines => "Máquinas",
        TabReleases => "Lanzamientos",

        // Stats blocks
        StatsIdentity => "[ Identidad ]",
        StatsRank => "  Rango       : ",
        StatsTitle => "  Título      : ",
        StatsCountry => "  País        : ",
        StatsLoved => "  Favoritos   : ",
        StatsAchievements => "[ Logros ]",
        StatsPoints => "  Puntos        : ",
        StatsTotalRoots => "  Roots totales : ",
        StatsTotalUsers => "  Users totales : ",
        StatsFirstRoots => "  FirstRoots    : ",
        StatsFirstUsers => "  FirstUsers    : ",
        StatsChallenges => "  Challenges    : ",
        StatsWriteups => "  Writeups      : ",
        StatsTrophies => "[ Trofeos ] ({})",
        StatsProgress => "[ Progreso ]",

        // Filter block
        FilterLabel => " filtro: ",
        PwnedHiddenIndicator => " · PWNED ocultas (h)",

        // Footer fragments
        FooterJkMove => "jk mover",
        FooterFilter => "/ filtrar",
        FooterEnterOpen => "Enter abrir",
        FooterWriteup => "u writeup",
        FooterSizeSort => "s orden tamaño",
        FooterHidePwned => "h ocultar PWNED",
        FooterFlag => "f flag",
        FooterDownload => "d descargar",
        FooterAccount => "a cuenta",
        FooterRefresh => "r refrescar",
        FooterQuit => "q salir",
        FooterTabSwitch => "Tab pestañas",
        FooterLangHint => "l idioma",
        FooterFilterMode => "Enter confirmar · Esc limpiar y salir del filtro",
        TabSubmissions => "Envíos",
        ColLevel => "Nivel",
        ColUser => "Usuario",
        SubmitFormTitle => " Envía tu VM ",
        SubmitName => "Nombre de la VM:",
        SubmitUrl => "URL de descarga:",
        SubmitUserFlag => "Flag user:",
        SubmitRootFlag => "Flag root:",
        SubmitWriteup => "URL del writeup:",
        SubmitTags => "Tags:",
        SubmitNotes => "Notas (opcional):",
        SubmitFormNotice => "Lee las reglas (i) antes de enviar.",
        SubmitFormHint => "↑↓/Tab campo · nivel cicla con Tab · Enter enviar · Esc cancelar",
        SubmitLevel => "Nivel:",
        SubmitOk => "[✓] VM enviada — esperando en la cola.",
        SubmitFailed => "[!] Envío rechazado: {}",
        SubmitUnknown => "[?] Respuesta desconocida: {}",
        SubmitRequiredMissing => "Faltan campos obligatorios: {}",
        SubmitInvalidLevel => "El nivel debe ser easy, medium o hard.",
        RulesTitle => " Reglas de envío ",
        RulesUnavailable => "Reglas no disponibles — prueba a refrescar.",
        FetchingForm => "Cargando formulario de envío...",
        FooterSubmitVm => "v enviar VM",
        FooterRules => "i reglas",
        MySubmissionsTitle => " Mis envíos ",
        QueueTitle => " Cola ",
        StatusSubmitOnlySubmissions => {
            "Las acciones de envío solo están disponibles en la pestaña Envíos."
        }
        CompactUser => "User",
        CompactRoot => "Root",
        CompactMachineNotFound => "máquina no encontrada",
        CompactUnknown => "desconocido",

        // Popups
        PopupSubmitFlags => " Enviar flags — {} ",
        FlagResultsTitle => " Resultado de flags — {} ",
        WriteupResultsTitle => " Resultado del writeup — {} ",
        PopupSubmitWriteup => " Enviar writeup — {} ",
        PopupDownload => " Descarga — {} ",
        PopupConfigure => " Configurar HackMyVM ",
        PopupUserFlag => "Flag user:",
        PopupRootFlag => "Flag root:",
        PopupWriteupUrl => "URL del writeup:",
        PopupSaveTo => "Guardar en:",
        PopupUsername => "Usuario:",
        PopupPassword => "Contraseña:",
        PopupHintFlag => "Enter enviar ambas · ↑↓/Tab cambiar campo · Esc cancelar",
        PopupHintSend => "Enter enviar · Esc cancelar",
        PopupHintDownload => "Tab completar ruta · Enter iniciar · Esc cancelar",
        PopupHintConfig => "Enter guardar y conectar · ↑↓/Tab cambiar campo · Esc salir",
        PopupDirectories => "  directorios:",
        PopupAccountTitle => " Cuenta — {} ",
        PopupLoggedInAs => "Sesión iniciada como {}",
        PopupAccountHint => "Enter cambiar cuenta · l cerrar sesión · Esc cerrar",
        PopupPwnedTitle => " ✓ Ya PWNED — {} ",
        PopupPwnedLine1 => "Las flags user y root ya están.",
        PopupPwnedLine2 => "Reenvío deshabilitado.",
        PopupPwnedClose => "Enter / Esc cerrar",
        NoticeFirstRun => "Primer arranque — introduce tu cuenta de HackMyVM.",
        NoticeLoginFailed => "Login fallido — vuelve a introducir tus credenciales de HackMyVM.",
        NoticeSwitch => "Cambiar de cuenta — introduce las nuevas credenciales.",
        NoticeLoggedOut => "Sesión cerrada — inicia sesión con tu cuenta de HackMyVM.",
        NoticeOneFlagRemains => "Ya se envió una flag — falta una.",
        ReportHintRefresh => "Los datos se actualizarán al cerrar · Enter / Esc cerrar",
        ReportHintClose => "Enter / Esc cerrar",
        DirectoriesMore => "  … y {} más",

        // Writeups popup
        WriteupsPopupTitle => " Writeups — {} ",
        WriteupsPopupHint => "Enter abrir enlace · jk seleccionar · Esc cerrar",
        ColDate => "Fecha",
        ColAuthor => "Autor (Poet)",
        ColLanguage => "Idioma",
        ColFormat => "Formato",
        ColLink => "Enlace",

        // Table headers
        ColVm => "VM",
        ColDifficulty => "Dificultad",
        ColCreator => "Creador",
        ColSize => "Tamaño",
        ColOs => "SO",
        ColCompat => "Probada",
        ColStatus => "Estado",
        NoCompat => "-",

        // Downloads overlay
        DownloadsTitle => " Descargas ",
        DownloadsEmpty => "Aún no hay descargas — pulsa d en una máquina.",
        DownloadsResolving => "… {}  resolviendo enlace de MEGA…",
        DownloadsHint => {
            "o cerrar (las descargas siguen) · c cancelar la última · q avisa mientras haya activas"
        }

        // Statuses — generic
        StatusCancelled => "Cancelado.",
        StatusBusy => "Ocupado — espera la operación actual.",
        StatusDataRefreshed => "Datos actualizados.",
        StatusFetchFailed => "Error al obtener datos: {}",
        StatusActionFailed => "Acción fallida: {}",
        StatusNothingSelected => "Nada seleccionado para actuar.",
        StatusNothingToInspect => "Nada seleccionado para inspeccionar.",
        StatusOpenedBrowser => "Abierto en el navegador: {}",
        StatusXdgOpenFailed => "xdg-open falló: {}",
        StatusConnecting => "Conectando...",
        StatusConnectedAs => "[✓] Conectado como {} — cargando datos...",
        StatusConfigFailed => "Configuración fallida: {}",
        StatusLoggedOut => "[✓] Sesión cerrada — introduce otra cuenta o pulsa Esc para salir.",
        StatusLogoutFailed => "Error al cerrar sesión: {}",
        StatusCredentialsRequired => "Se requieren usuario y contraseña.",
        StatusEmptyInput => "Cancelado — entrada vacía.",
        StatusAlreadyPwned => "{} ya está PWNED — nada que enviar.",
        StatusCancelDownload => "Cancelando {}...",

        // Statuses — machines tab
        StatusHidePwnedOnly => "Ocultar PWNED solo está disponible en la pestaña Máquinas.",
        StatusPwnedHidden => "Máquinas PWNED ocultas — pulsa h para mostrarlas otra vez.",
        StatusPwnedShown => "Máquinas PWNED visibles.",
        StatusSortOnly => "El orden por tamaño solo está disponible en la pestaña Máquinas.",
        StatusFlagOnlyMachines => "El envío de flags solo está disponible en la pestaña Máquinas.",
        StatusDownloadOnlyMachines => {
            "Las descargas solo están disponibles en la pestaña Máquinas."
        }
        StatusUploadOnlyPending => {
            "El envío de writeups solo está disponible en la pestaña Pendientes."
        }
        StatusWriteupsTabs => {
            "Los writeups están disponibles en las pestañas Máquinas y Pendientes."
        }

        // Statuses — popups & downloads
        StatusLangSwitched => "Idioma cambiado a {}. · l para volver a cambiar",
        StatusDownloadStarted => "[↓] Descarga de {} iniciada.",
        StatusDownloadQueued => "[↓] {} en cola — {} descargas activas.",
        StatusDownloadFailed => "Descarga fallida: {}",
        StatusQueuedFlag => "Flag en cola para {}...",
        StatusQueuedFlags => "Flags en cola para {}...",
        StatusQueuedWriteup => "Writeup en cola para {}...",
        StatusQueuedDownload => "Descarga en cola para {}...",
        StatusNoWriteups => "No se encontraron writeups de la comunidad para {}.",

        // Report entries
        ReportFlagAccepted => "{}: ✓ ACEPTADA",
        ReportFlagRejected => "{}: ✗ RECHAZADA",
        ReportMachineNotFound => "Máquina '{}' no encontrada",
        ReportUnknownResponse => "Respuesta desconocida: {}",
        ReportYouHacked => "[✓] ¡Has hackeado {}!",
        ReportWriteupAccepted => "Writeup: ✓ ACEPTADO — {}",
        QuitWarnDownloads => "{} descarga(s) activa(s) — pulsa q otra vez para abortar: {}",
        FetchingLoading => "Cargando datos...",
        FetchingRefreshing => "Actualizando datos...",
        FetchingLogout => "Cerrando sesión...",
        FetchingFlag => "Enviando flag de {}...",
        FetchingWriteup => "Enviando writeup de {}...",
        FetchingWriteupsList => "Cargando writeups de {}...",
        ReportWriteupRepeated => "Writeup: [=] YA ENVIADO",
        ReportWriteupRejected => "Writeup: ✗ RECHAZADO — ¿faltan flags?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_roundtrip() {
        assert_eq!(Lang::En.other(), Lang::Es);
        assert_eq!(Lang::Es.other(), Lang::En);
    }

    #[test]
    fn config_parsing() {
        assert_eq!(Lang::from_config("es"), Lang::Es);
        assert_eq!(Lang::from_config("ES"), Lang::Es);
        assert_eq!(Lang::from_config("en"), Lang::En);
        assert_eq!(Lang::from_config(""), Lang::En);
        assert_eq!(Lang::from_config("fr"), Lang::En);
        assert_eq!(Lang::Es.to_config(), "es");
    }

    #[test]
    fn languages_differ_on_sample_keys() {
        let keys = [
            Key::TabMachines,
            Key::StatusPwnedHidden,
            Key::ColDifficulty,
            Key::FooterQuit,
            Key::PopupUserFlag,
        ];
        for key in keys {
            assert_ne!(
                Lang::En.t(key),
                Lang::Es.t(key),
                "{key:?} is identical in both languages"
            );
        }
    }
}
