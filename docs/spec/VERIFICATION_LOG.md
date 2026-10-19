# سجل التحقق (VERIFICATION_LOG)

قاعدة: كل مدخل يسجل ما حدث فعلًا. الحقول المجهولة تكتب «غير متاح».

## VL-0001 — فحص محتويات الأرشيفين
- التاريخ: 2026-10-18 (الوقت: غير مسجل). Commit: **غير متاح** (لا يوجد `.git` في الأرشيف).
- البيئة: حاوية Linux x86_64، بلا شبكة.
- الأمر: `unzip -l qessa-docs.zip`، `tar -tf qessa-source__1__tar.gz` ثم استخراج.
- النتيجة: نجح. الوثائق 10 ملفات (~70 KB). المصدر 52 مدخلًا. حجم الشيفرة: Rust ≈ 5,130 سطرًا، `qessac.qs` 2,842، `stdlib.qs` 604، `tests/cli.rs` 653.
- الحالة: ناجح.

## VL-0002 — محاولة بناء المشروع
- الأمر: `cargo --version` و`find / -name rustc -type f`.
- النتيجة: **فشل** (`cargo: not found`، ولا يوجد rustc).
- أثره: لم يُنفذ `cargo test --locked` ولا `cargo build --release --locked` ولا bootstrap. جميع الأرقام التالية «مدّعاة في الوثائق ولم يتحقق منها الوكيل»: 13/13 نجاح، 22 تحذيرًا، 157 موضع فروق تنسيق، تطابق stage-2/3.
- الحالة: **BLOCKED** (OQ-4).

## VL-0003 — فحوص ثابتة على المصدر (قراءة وعدّ)
| الفحص | الأمر/الطريقة | النتيجة |
|---|---|---|
| عدد syscalls في Rust | عدّ `"syscall_` في `syscall_number` | 45 |
| أسماء syscall في selfhost | قراءة `syscall_num` (`qessac.qs:1612`) | 7: read, write, open, close, mmap, exit, getpid |
| fallback المجهول في selfhost | قراءة `qessac.qs:1612-1621` و`2018-2025` | يعيد/يضبط 60 (exit) |
| عدد الاختبارات | `grep -c '^#\[test\]' tests/cli.rs` | 13 |
| اتجاه `cg_lookup_local` | قراءة `qessac.qs:1264-1277` | من الأحدث إلى الأقدم |
| معرّف مجهول في selfhost | قراءة `qessac.qs:1676-1708` | يُترجم إلى 0 |
| إسناد/عنوان متغير مجهول | قراءة `qessac.qs:1734-1760` | يُتجاهل / يصبح 0 |
| دالة مجهولة في selfhost | قراءة `qessac.qs:2044-2068` | يُولَّد `ud2` |
| Match/Closure/MacroCall في Rust | قراءة `codegen/mod.rs:790` | يُولَّد `mov rax,0` بلا خطأ |
| حد bump في `__alloc_array` | قراءة `emit/mod.rs:129-137` | لا مقارنة بالحد |
| إصدارات | `Cargo.toml` و`qessac.qs:2765` | الحزمة 3.0.0، الذاتي يطبع v2.2 |
| unwrap/panic/expect | `grep -c` | lexer: 1، parser: 4، البقية 0 |
- الحالة: ناجح كقراءة. لا تُحسب كاختبارات تشغيل.

## قرار البوابة الحالية
- بوابة المرحلة A (الحالة الأساسية): **غير مجتازة**. السبب: لا Git، ولا Rust، فلا خط أساس قابل للتكرار. والمطلوب لإغلاقها: OQ-4 وOQ-5.

## VL-0004 — تعديلات M1 (كتابة فقط، لم تُترجم ولم تُشغَّل)
- التاريخ: 2026-10-18. Commit: غير متاح (لا Git). البيئة: نفس VL-0002 (بلا Rust).
- التغييرات (وفق `m1.patch`، 42 سطرًا مضافًا/محذوفًا في المصدر + اختباران جديدان):
  1. `qessac.qs`: syscall مجهول ⇒ `error: unknown syscall '<name>'` وخروج 1 بدل التحويل إلى 60؛ و`syscall_num` يعيد -1.
  2. `qessac.qs`: دالة مجهولة أو helper وقت التشغيل غير معروف ⇒ خطأ ترجمة وخروج 1 بدل توليد `ud2`.
  3. `codegen/mod.rs`: `Match`/`Closure`/`MacroCall` في موضع تعبير ⇒ خطأ `unsupported ...` بدل `mov rax,0`.
  4. `codegen/mod.rs`: نمط `Str` كان يطابق أي قيمة غير فارغة (لا يقارن المحتوى)، ونمط `Tuple` لا يطابق أبدًا ⇒ كلاهما خطأ `unsupported`. (اكتشاف جديد في هذا الفحص.)
  5. `tests/cli.rs`: اختباران جديدان: `rust_compiler_rejects_unimplemented_constructs_instead_of_compiling_them_to_zero` و`self_hosted_compiler_rejects_unknown_syscalls_and_functions`.
- فحوص ثابتة أجريتها قبل التعديل (سكربت Python على المصدر): `qessac.qs` وstdlib لا يستدعيان أي دالة غير معرّفة، ولا يستخدمان syscall خارج السبعة المعرّفة؛ الأمثلة وملفات الاختبار لا تستخدم match-تعبيرًا ولا closures ولا macros ولا أنماط string/tuple. هذا يقلل احتمال كسر bootstrap، **ولا يثبت** عدم كسره.
- فحص بنيوي: توازن الأقواس في الملفات المعدلة يطابق الأصل. ليس فحص ترجمة.
- **غير منفذ:** `cargo build`، `cargo test`، bootstrap. الحالة: `IMPLEMENTED_UNVERIFIED`.
- مخاطر معروفة: (أ) صياغة closure في الاختبار الجديد (`|x| x + 1`) استنتجتها من parser ولم أشغّلها؛ (ب) خروج selfhost يطبع الخطأ على stdout لا stderr (يفحصه الاختبار على الاثنين)؛ (ج) أي استدعاء دالة مجهولة كان مخفيًا في مسار لم أفحصه سيكسر bootstrap الآن.
- **لم يُعالج بعد (QES-SEC-002 مفتوح):** معرّف مجهول في selfhost يصبح 0، وإسناد/عنوان متغير مجهول يُتجاهل (يتطلب علامة «موجود» لأن `cg_lookup_const` يعيد 0 لثابت قيمته 0، مثل `RAX`)؛ وأخطار allocator (QES-MEM-001/002) لأن حسابها مشفّر بإزاحات بايتات يدوية في Rust وselfhost ولا أعدّلها دون مترجم.

## VL-0005 — إصلاح المعرّف المجهول في selfhost + فاحص جداول syscall
- التاريخ: 2026-10-19. Commit: غير متاح. البيئة: حاوية Linux x86_64، Python 3 متاح، Rust غير متاح.
- التغييرات (كتابة، لم تُترجم):
  1. `qessac.qs`: دالتان جديدتان `cg_const_exists` و`cg_fail_unknown_identifier`؛ معرّف مجهول، أو إسناد إلى متغير مجهول، أو `&x` لمتغير مجهول ⇒ `error: unknown identifier '<name>'` وخروج 1. الثابت صفر القيمة (مثل `RAX`) لا يُرفض.
  2. `tests/cli.rs`: حالتان سلبيتان إضافيتان (معرّف وإسناد مجهولان) وحالة إيجابية لثابت قيمته 0 (`let ZERO = 0`) داخل `self_hosted_compiler_rejects_unknown_syscalls_and_functions`.
  3. `tools/check_syscall_tables.py`: فاحص جدول syscall.
- **أمر منفذ فعلًا:** `python3 tools/check_syscall_tables.py`
  - النتيجة: `rust: 45 syscalls; selfhost: 7 syscalls (missing in selfhost: 38)` ثم `OK`، رمز الخروج 0.
  - يتحقق من: (أ) أرقام selfhost السبعة تطابق Rust، (ب) لا قيمة افتراضية صالحة في selfhost، (ج) أرقام Rust الخمسة والأربعون تطابق قائمة Linux x86-64 المرجعية داخل السكربت، (د) اتساق جدولي selfhost الداخليين.
  - **اختبار سلبي منفذ:** بإعادة `let num = 60` مؤقتًا أخفق الفاحص (`defaults to 60`، رمز الخروج 1)، ثم أعدت الملف ونجح. هذا يثبت أن الفاحص يكشف الخلل القديم.
- قيود: القائمة المرجعية للأرقام نقلتها بنفسي من معرفتي بجدول النواة ولم أقارنها بمصدر النواة هنا (لا شبكة)؛ تحتاج مراجعة قبل الإصدار. وهذا الفاحص يؤكد الاتساق الثابت فقط، لا سلوك المترجم وقت التشغيل.
- أثره على المتطلبات: QES-SEC-001 لديه الآن فحص ثابت منفذ (تحقق جزئي)، لكنه يبقى `IMPLEMENTED_UNVERIFIED` حتى يعمل اختبار cargo. المتبقي منه: ولّد جدول Rust وselfhost من مصدر واحد (`syscalls.def`)، وأضف 38 syscall مفقودة إلى selfhost أو وثّق أنها خارج selfhost.

## VL-0006 — إضافة CI على GitHub Actions (كتابة فقط)
- التاريخ: 2026-10-19. الملف: `.github/workflows/ci.yml`. لم يُشغَّل (لا مستودع GitHub متاح لي).
- الفحص الوحيد المنفذ: تحليل صياغة YAML بـ Python (إن توفر `pyyaml`)، وهو لا يثبت أن الـ workflow يعمل.
- يشغّل على `ubuntu-24.04` (x86-64): فاحص syscall، `cargo build --release --locked`، `cargo test --locked`، اختباري M1 صراحةً، bootstrap، فحص إعادة إنتاج البناء (معلوماتي)، fmt/clippy (معلوماتي لأن 157 فرق تنسيق معروفة)، ويرفع مجلد `logs/` كـ artifact باسم `test-logs`.
- يغطي جزءًا من QES-TEST-001 وQES-BOOT-001 وQES-REL-001 فقط. لا يغطي ARM64/macOS/Windows (backends غير موجودة)، ولا fuzz، ولا differential. ويبقى كل ذلك `APPROVED` غير منفذ.
