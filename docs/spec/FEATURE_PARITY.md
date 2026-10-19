# مصفوفة الميزات (FEATURE_PARITY)

الرموز: ✔ = مثبت بقراءة الشيفرة ولم يُشغَّل بعد (لا Rust)، ✘ = غير موجود، ⚠ = موجود جزئيًا أو يخفي فشلًا، ؟ = لم أتحقق. لا ميزة هنا بحالة `VERIFIED`.

| الميزة | مواصفة | Rust parse | Rust sema | Rust codegen | Selfhost parse | Selfhost sema/codegen | Runtime | اختبار | الحالة |
|---|---|---|---|---|---|---|---|---|---|
| دوال/معاملات ≤ 6 | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | موجود (وثائقيًا) | IMPLEMENTED_UNVERIFIED |
| معاملات > 6 (stack) | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | موجود (وثائقيًا) | IMPLEMENTED_UNVERIFIED |
| let/while/for/if/break | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | أمثلة | IMPLEMENTED_UNVERIFIED |
| مصفوفات | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ⚠ (حد التخصيص) | مثال arrays | IN_PROGRESS |
| مؤشرات | ✔ | ✔ | ✔ | ✔ | ✔ | ⚠ (`&x` المجهول=0) | — | مثال pointers | IN_PROGRESS |
| syscalls | ✔ | ✔ | ✔ | ✔ (45) | ✔ | ⚠ (7، المجهول=exit) | ✔ | جزئي | BLOCKED على QES-SEC-001 |
| struct | ✔ | ✔ | ✔ | ✔ | ✘ | ✘ | — | مثال structs | IN_PROGRESS |
| enum | ✔ | ✔ | ✔ | ✔ | ✘ | ✘ | — | مثال enums | IN_PROGRESS |
| modules / use | ✔ | ✔ | ✔ | ✔ | ✘ | ✘ | — | مثال modules | IN_PROGRESS |
| regions | ✔ | ✔ | ✔ | ✔ | ✘ | ✘ | ⚠ (بلا فحص حد) | مثال regions | IN_PROGRESS |
| match | ✔ | ✔ | ؟ | ⚠ (يولّد 0) | ✘ | ✘ | — | لا | APPROVED |
| closures | ✔ | ✔ | ؟ | ⚠ (يولّد 0) | ✘ | ✘ | — | لا | APPROVED |
| macros | ✔ | ✔ | ⚠ (stub) | ⚠ (يولّد 0) | ✘ | ✘ | — | لا | APPROVED |
| generics | ✔ | ✔ | ✘ | ✘ | ✘ | ✘ | — | لا | APPROVED |
| floats | ✔ | ✔ | ✘ | ⚠ (bits فقط) | ✘ | ✘ | — | لا | APPROVED |
| methods | ✔ | ✔ | ✔ | ⚠ (helpers) | ✘ | ✘ | — | لا | APPROVED |
| tuples | ✔ | ✔ | ✘ | ⚠ | ✘ | ✘ | — | لا | APPROVED |
| extern | ✔ | ✔ | ✔ | ✔ (يرفض غير syscall) | ✘ | ✘ | — | جزئي | APPROVED |
| `new` | ✔ | ✔ | ؟ | ✔ | ✘ | ✘ | — | parser فقط | APPROVED |
| أنواع اختيارية (D-004) | ✘ | ✘ | ✘ | ✘ | ✘ | ✘ | — | لا | APPROVED |
| ذاكرة تلقائية (D-005) | ✘ | ✘ | ✘ | ✘ | ✘ | ✘ | ✘ | لا | BLOCKED على OQ-2 |
| backend ARM64/macOS/Windows | ✘ | — | — | ✘ | — | ✘ | — | لا | BLOCKED على OQ-1، OQ-4 |

قاعدة الإكمال: لا تصبح الميزة `VERIFIED` إلا بتحقق كل أعمدة المترجمين والمنصات المعلنة وبوجود اختبار إيجابي وسلبي وحدودي.
