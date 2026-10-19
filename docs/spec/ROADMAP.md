# خارطة الطريق (ROADMAP)

لا أقدّر مددًا زمنية: لا أملك بيئة تنفيذ ولا بيانات إنتاجية. الترتيب بحسب الاعتماديات.

| المعلم | المحتوى | يعتمد على | معيار الإنهاء | المخاطر |
|---|---|---|---|---|
| M0 خط الأساس | Git، بيئة Rust، تشغيل الاختبارات الحالية، CI هيكلي، تسجيل الأرقام الفعلية | OQ-4، OQ-5 | بوابة A | بدون Rust لا يتقدم شيء |
| M1 السلامة P0 | `syscalls.def` موحد؛ رفض المجهول في selfhost (معرّف/دالة/syscall)؛ إعادة كتابة `__alloc_array` مع فحوص؛ canaries | M0 | بوابة B | تغيير runtime يمس كل البرامج؛ يتطلب bootstrap كامل |
| M2 واجهة موحدة وتشخيصات | spans، sema إلزامي في selfhost، `unsupported`، إزالة unwrap/panic، جدول الأسبقية | M1، OQ-3 | بوابة C جزئيًا | selfhost يحتاج توسيعًا كبيرًا |
| M3 الميزات الأساسية | struct/enum/modules/regions/methods/tuples/new في selfhost + عقد | M2 | AC-04 للأساس | ثمن parity |
| M4 الميزات المتقدمة والأنواع | match، closures، macros، generics، floats، أنواع اختيارية | M3، OQ-6 | AC-22 | generics وclosures تحتاج قرارات تصميم كبيرة |
| M5 الذاكرة التلقائية | تصميم OQ-2 ثم تنفيذه | M4 والموافقة | AC-23 | الأعلى مخاطرة في اللغة |
| M6 Backends | IR وسيط (بموافقة)، ARM64 Linux، ثم macOS وWindows | OQ-1، OQ-4، موافقة IR | AC-06/10/11 لكل هدف | Windows/macOS: ABI وتوقيع واستيراد |
| M7 الأدوات | formatter، linter، REPL، debugger، LSP، package manager | M2-M4 | AC-17 | الأدوات تعتمد على نحو مستقر |
| M8 التحقق والتوزيع | AC-13…20، 24، artifacts، إعادة الإنتاج، وثائق | الكل | بوابة F وقائمة الإصدار | |

الأولوية الحالية: M0 ثم M1. إن لم يتوفر Rust فيُنفَّذ M1 كتعديلات غير مختبرة (`IMPLEMENTED_UNVERIFIED`) مع تحذير صريح، وهذا غير مفضل.
