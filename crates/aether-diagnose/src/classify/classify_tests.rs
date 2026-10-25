use super::*;

fn c(text: &str) -> LocalDiagnosis { classify_locally(text, &[], None).expect("match") }
fn has_action(d: &LocalDiagnosis, action: SuggestedAction) -> bool { d.actions.contains(&action) }

#[test] fn unrecognized_option_positive() { let d = c("Unrecognized option: --sun-misc-unsafe-memory-access=allow"); assert_eq!(d.confidence, Confidence::High); assert!(has_action(&d, SuggestedAction::SwitchJava { target: "Java 23".into() })); }
#[test] fn unrecognized_option_negative() { assert!(classify_locally("Using option --sun-misc-unsafe-memory-access=allow", &[], None).is_none()); }
#[test] fn unrecognized_option_context() { let d = classify_locally("Unrecognized option: --sun-misc-unsafe-memory-access=allow", &["--sun-misc-unsafe-memory-access=allow".into()], Some(17)).unwrap(); assert!(d.title_zh.contains("23")); }
#[test] fn jvm_create_positive() { assert_eq!(c("Error: Could not create the Java Virtual Machine.").cause_id, "unrecognized_option"); }
#[test] fn fatal_vm_positive() { assert_eq!(c("Error: A fatal exception has occurred. Program will exit.").cause_id, "unrecognized_option"); }
#[test] fn jvm_negative() { assert!(classify_locally("The Java Virtual Machine started", &[], None).is_none()); }

#[test] fn resolution_positive() { let d = c("net.minecraftforge.fml.ModLoadingException: Missing or unsupported mandatory dependencies"); assert_eq!(d.confidence, Confidence::High); assert!(has_action(&d, SuggestedAction::ExportDiagnostics)); assert!(matches!(d.actions[0], SuggestedAction::DisableMod { .. })); }
#[test] fn resolution_negative() { assert!(classify_locally("ModLoading completed successfully", &[], None).is_none()); }
#[test] fn neoforge_resolution() { assert_eq!(c("NeoForge dependency resolution failed").cause_id, "mod_resolution_exception"); }

#[test] fn oom_heap() { let d = c("java.lang.OutOfMemoryError: Java heap space"); assert_eq!(d.confidence, Confidence::High); assert!(matches!(d.actions[0], SuggestedAction::IncreaseMemory { .. })); }
#[test] fn oom_metaspace() { assert_eq!(c("java.lang.OutOfMemoryError: Metaspace").cause_id, "out_of_memory"); }
#[test] fn oom_gc() { assert_eq!(c("GC overhead limit exceeded").cause_id, "out_of_memory"); }
#[test] fn oom_negative() { assert!(classify_locally("Memory allocation completed", &[], None).is_none()); }

#[test] fn mixin_positive() { let d = c("Mixin apply failed for mod example"); assert_eq!(d.confidence, Confidence::High); assert!(matches!(d.actions[0], SuggestedAction::DisableMod { .. })); }
#[test] fn mixin_error_positive() { assert_eq!(c("MixinApplyError: bad injection").cause_id, "mixin_apply_failed"); }
#[test] fn mixin_transformer_positive() { assert_eq!(c("org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError").cause_id, "mixin_apply_failed"); }
#[test] fn mixin_negative() { assert!(classify_locally("Mixin config loaded", &[], None).is_none()); }

#[test] fn opengl_positive() { let d = c("GLFW error 65542: WGL: The driver does not appear to support OpenGL"); assert_eq!(d.confidence, Confidence::High); assert!(has_action(&d, SuggestedAction::ExportDiagnostics)); assert!(has_action(&d, SuggestedAction::ViewLogs)); }
#[test] fn opengl_window() { assert_eq!(c("Failed to create window").cause_id, "opengl_or_driver"); }
#[test] fn opengl_version() { assert_eq!(c("OpenGL 3.2 or later is required").cause_id, "opengl_or_driver"); }
#[test] fn opengl_pixel() { assert_eq!(c("Pixel format not accelerated").cause_id, "opengl_or_driver"); }
#[test] fn opengl_negative() { assert!(classify_locally("OpenGL renderer initialized", &[], None).is_none()); }

#[test] fn missing_dependency_positive() { let d = c("Mod 'alpha' requires 'beta' of version 1.2.3"); assert_eq!(d.confidence, Confidence::High); assert!(matches!(d.actions[0], SuggestedAction::DisableMod { .. })); assert!(has_action(&d, SuggestedAction::ExportDiagnostics)); }
#[test] fn incompatible_mod_set() { assert_eq!(c("Incompatible mod set!").cause_id, "missing_mod_dependency"); }
#[test] fn fabric_formatted() { assert_eq!(c("net.fabricmc.loader.impl.FormattedException: dependency").cause_id, "missing_mod_dependency"); }
#[test] fn mods_toml_missing() { assert_eq!(c("mods.toml missing").cause_id, "missing_mod_dependency"); }
#[test] fn missing_dependency_negative() { assert!(classify_locally("All mods loaded", &[], None).is_none()); }

#[test] fn class_version_55() { let d = c("UnsupportedClassVersionError: compiled by a more recent version (class file version 55.0)"); assert_eq!(d.confidence, Confidence::High); assert!(has_action(&d, SuggestedAction::SwitchJava { target: "Java 11".into() })); }
#[test] fn class_version_67() { let d = c("Unsupported class file major version 67"); assert!(has_action(&d, SuggestedAction::SwitchJava { target: "Java 23".into() })); }
#[test] fn class_version_negative() { assert!(classify_locally("class file version 52 is supported", &[], None).is_none()); }

#[test] fn auth_invalid() { let d = c("Invalid credentials"); assert_eq!(d.confidence, Confidence::High); assert!(has_action(&d, SuggestedAction::ViewLogs)); assert!(has_action(&d, SuggestedAction::ExportDiagnostics)); }
#[test] fn auth_expired() { assert_eq!(c("token expired").cause_id, "auth_token_expired"); }
#[test] fn auth_401() { assert_eq!(c("401 Unauthorized").cause_id, "auth_token_expired"); }
#[test] fn auth_exception() { assert_eq!(c("com.mojang.authlib.exceptions.AuthenticationException").cause_id, "auth_token_expired"); }
#[test] fn auth_negative() { assert!(classify_locally("Authentication succeeded", &[], None).is_none()); }

#[test] fn priority_java_before_oom() { let d = c("Unrecognized option: --sun-misc-unsafe-memory-access=allow\njava.lang.OutOfMemoryError: Java heap space"); assert_eq!(d.cause_id, "unrecognized_option"); }
#[test] fn evidence_cap() { let text = (0..10).map(|_| "GLFW error 65542").collect::<Vec<_>>().join("\n"); assert_eq!(c(&text).evidence_lines.len(), 5); }
#[test] fn evidence_is_bounded() { let d = c(&format!("GLFW error 65542 {}", "x".repeat(500))); assert!(d.evidence_lines[0].len() <= 300); }
#[test] fn redact_control_and_cap() { let out = redact_log_line("abc\u{1b}[31m\n\tdef"); assert!(!out.contains('\u{1b}')); assert!(!out.contains('\n')); assert!(out.contains('\t')); assert!(redact_log_line(&"x".repeat(400)).len() <= 300); }
#[test] fn unknown_is_none() { assert!(classify_locally("A completely unrelated normal log", &[], None).is_none()); }
#[test] fn confident_only_high() { assert!(c("Invalid credentials").confident()); }
#[test] fn rules_have_expected_ids() { assert!(RULES.iter().any(|r| r.id == "unrecognized_option")); assert!(RULES.windows(2).all(|w| w[0].priority < w[1].priority)); }
