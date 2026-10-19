# سجل المتطلبات والتتبع (TRACEABILITY)

الحالات: `PROPOSED` `APPROVED` `IN_PROGRESS` `IMPLEMENTED_UNVERIFIED` `VERIFIED` `BLOCKED` `DEFERRED` `REJECTED`.
`APPROVED` = ناتج عن قرار المستخدم أو عن معيار قبول وافق عليه. لا ينتقل متطلب إلى `VERIFIED` دون دليل في `VERIFICATION_LOG.md`. النسب في آخر الملف **محسوبة بسكربت** من هذا الجدول.

| المعرف | الوصف | الأولوية | المصدر | الملفات المعنية | الاختبارات المطلوبة | المنفذة | الدليل | الحالة |
|---|---|---|---|---|---|---|---|---|
| QES-SPEC-001 | مصدر حقيقة وحيد في docs/spec | P0 | D-006 | docs/spec/* | مراجعة المستخدم | لا | VL-0001 | IMPLEMENTED_UNVERIFIED |
| QES-BASE-001 | Git وبيئة Rust وخط أساس مسجل | P0 | بوابة A | المستودع | تشغيل cargo test/build | لا | VL-0002 | BLOCKED (OQ-4، OQ-5) |
| QES-SEC-001 | منع تحويل syscall مجهول إلى عملية أخرى؛ جدول واحد | P0 | D-006، تدقيق | codegen/mod.rs، qessac.qs، syscalls.def | AC-01، AC-21 | جزئيًا (selfhost) + فاحص ثابت منفذ | VL-0003، VL-0004، VL-0005 | IMPLEMENTED_UNVERIFIED |
| QES-SEC-002 | لا silent fallback: معرّف/دالة/إسناد مجهول، match/closure/macro | P0 | D-006، AC-01 | codegen/mod.rs:790، qessac.qs:1676-2068 | AC-01، AC-21 | جزئيًا (selfhost: معرّف/دالة؛ Rust: match/closure/macro/أنماط) | VL-0003، VL-0004، VL-0005 | IN_PROGRESS |
| QES-MEM-001 | فحص حدود التخصيص وchecked arithmetic | P0 | AC-03، AC-05 | emit/mod.rs:101-160 | AC-03، AC-21 | لا | VL-0003 | APPROVED |
| QES-MEM-002 | عقد موحد لفشل mmap والتخصيص في stdlib | P0 | AC-05 | stdlib.qs | AC-03 | لا | VL-0003 | APPROVED |
| QES-MEM-003 | ذاكرة تلقائية ذكية + تحكم كامل | P1 | D-005 | runtime، sema | AC-23 | لا | — | BLOCKED (OQ-2) |
| QES-ABI-001 | ABI موثق ومختبر لعدد وسائط أكبر وتداخل | P1 | AC-10 | codegen، qessac.qs | حدود وسائط | لا | — | APPROVED |
| QES-COMP-001 | تطابق Rust وselfhost على corpus المعلن | P1 | AC-04 | الكل | differential | لا | — | APPROVED |
| QES-BOOT-001 | bootstrap قابل لإعادة الإنتاج بلا فروق | P1 | AC-06، AC-07 | tests، CI | AC-06، AC-07 | لا | — | APPROVED |
| QES-BOOT-002 | selfhost يبني نفسه كاملًا | P1 | D-002 | qessac.qs | AC-06 | لا | — | APPROVED |
| QES-DIAG-001 | تشخيصات بـ spans دقيقة موحدة | P1 | AC-08 | lexer، parser، sema | golden | لا | — | APPROVED |
| QES-LANG-001 | عقد الحالات الحدية (overflow، قسمة، حدود، null…) | P1 | LANGUAGE_SPEC §2 | المواصفة والمترجمان | boundary | لا | — | PROPOSED |
| QES-LANG-002 | جدول أسبقية وترابط رسمي من parser | P1 | AC-08 | parser | golden | لا | — | APPROVED |
| QES-FEAT-001 | structs/enums/modules/regions/new في selfhost | P1 | D-011، AC-04 | qessac.qs | differential | لا | — | APPROVED |
| QES-FEAT-002 | match/closures/macros/generics/floats/methods/tuples end-to-end | P1 | D-011 | كل الطبقات | إيجابي/سلبي/حدودي | لا | — | BLOCKED (OQ-3، OQ-6) |
| QES-TYPE-001 | أنواع اختيارية يتحقق منها المترجم | P1 | D-004 | ast، sema | AC-22 | لا | — | BLOCKED (OQ-6) |
| QES-ARCH-001 | فصل ISA/OS/Container وIR وسيط | P1 | D-006، D-003 | codegen، emit | AC-06، AC-10 | لا | — | PROPOSED |
| QES-PLAT-001 | Linux ARM64 | P2 | D-003 | backend جديد | AC-06، AC-10، AC-11 | لا | — | BLOCKED (OQ-4) |
| QES-PLAT-002 | macOS (Mach-O) | P2 | D-003 | backend جديد | AC-06، AC-10، AC-11 | لا | — | BLOCKED (OQ-1، OQ-4) |
| QES-PLAT-003 | Windows (PE) | P2 | D-003 | backend جديد | AC-06، AC-10، AC-11 | لا | — | BLOCKED (OQ-1، OQ-4) |
| QES-TOOL-001 | Formatter وLinter | P2 | D-008 | أدوات جديدة | AC-14، AC-17 | لا | — | APPROVED |
| QES-TOOL-002 | REPL وDebugger | P2 | D-008 | أدوات جديدة | AC-17 | لا | — | APPROVED |
| QES-TOOL-003 | LSP ودعم المحرر ومدير الحزم | P2 | D-008 | أدوات جديدة | AC-17 | لا | — | APPROVED |
| QES-TEST-001 | corpus وdifferential وfuzz وCI | P1 | AC-02، AC-09 | tests، CI | AC-02، AC-09 | لا | — | APPROVED |
| QES-QUAL-001 | تحذيرات وfmt وclippy وتغطية وأداء | P2 | AC-13..16 | المشروع | AC-13..16 | لا | — | APPROVED |
| QES-REL-001 | توزيع binaries ومصدر وبناء قابل لإعادة الإنتاج وSBOM | P2 | D-009 | CI، release | AC-07، AC-19، AC-20 | لا | — | APPROVED |
| QES-REL-002 | توحيد إصدار الأداة والحزمة واللغة | P3 | تدقيق | Cargo.toml، qessac.qs:2765 | فحص آلي | لا | VL-0003 | APPROVED |
| QES-REL-003 | شروط الإصدار النهائي AC-01..24 | P0 | D-007، D-010 | الكل | AC-01..24 | لا | — | APPROVED |
| QES-DOC-001 | ربط الوثائق القديمة بالمرجع الجديد وتصحيح التناقضات | P2 | المرحلة 3 | docs/*.md، README.md | AC-18 | جزئيًا | هذا الملف | IN_PROGRESS |

## ملخص محسوب (سكربت على الجدول أعلاه)

| الحالة | العدد |
|---|---|
| APPROVED | 17 |
| BLOCKED | 7 |
| IMPLEMENTED_UNVERIFIED | 2 |
| IN_PROGRESS | 2 |
| PROPOSED | 2 |

المتطلبات الكلية: 30. المتحقق منها (VERIFIED): 0. نسبة التحقق = 0/30.
هذه ليست نسبة اكتمال اللغة، بل نسبة المتطلبات التي أُثبتت بأدلة.
