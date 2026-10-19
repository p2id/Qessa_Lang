# Qessa

مترجم لغة Qessa يولّد ملفات تنفيذية ELF مباشرة لمنصة Linux x86-64. تتضمن الشجرة مترجمًا تمهيديًا بـ Rust، ومترجمًا ذاتي الاستضافة بلغة Qessa، ومكتبة قياسية وأمثلة.

## الحالة

تُبنى حزمة Rust وتنجح **13 اختبار تكامل** وتشغيل الأمثلة المستقلة. يضمّن CLI المكتبة القياسية افتراضيًا، ويمرر `argv` ورمز الخروج، ويرفض رموزًا غير معرفة قبل إصدار ELF. اجتاز bootstrap المراحل stage-1 → stage-2 → stage-3، مع اختبار empty input ودوال 7 و8 معاملات وتطابق stage-2/3 بايتًا ببايت. أصلح البحث عن أحدث binding في selfhost، كما أُصلح ترتيب ومحاذاة وسائط stack.

اختُبرت المراحل الثلاث على ثمانية أمثلة ذاتية مدعومة مع تطابق stdout وstderr مع مترجم Rust. تنفيذ selfhost الحالي يرفض `comprehensive`, `enums`, `modules`, `regions`, و`structs` ولا ينبغي الخلط بين subset هذا وبين كامل ميزات Rust compiler.

يشمل bootstrap كذلك تشغيل `argc/argv` بلا وسيطات ومع وسيطين، وقراءة مصدر يتجاوز 1 MiB في stage-2 وstage-3، والتحقق من أن مدخلًا malformed يفشل بتشخيص دون إنشاء ملف تنفيذي.

**حدود الإثبات:** نجاح bootstrap مثبت للمسارات التي يغطيها الاختبار، وليس دليلًا على اكتمال اللغة أو استقرار production. ما تزال 22 تحذير Rust وفجوات تغطية ومصفوفة منصات قائمة. راجع [خطة التطوير](docs/DEVELOPMENT_PLAN.md) و[تقرير التنفيذ](docs/IMPLEMENTATION_REPORT.md).

## المتطلبات

- Rust وCargo (إصدار Rust 2021 أو متوافق).
- Linux x86-64 لتشغيل ملفات ELF الناتجة.

## البناء والاختبار

```bash
cargo test --locked
cargo build --release --locked
```

## الاستخدام

```bash
./target/release/qessac run examples/hello.qs
./target/release/qessac run --no-stdlib examples/hello.qs
./target/release/qessac compile examples/hello.qs -o hello
./hello
```

يُضمّن `stdlib.qs` تلقائيًا في `compile` و`run`. استخدم `--no-stdlib` لتعطيله. تمرر وسائط البرنامج بعد ملف Qessa:

```bash
./target/release/qessac run examples/cat.qs input.txt
./target/release/qessac run examples/wc.qs input.txt
```

أوامر أخرى: `tokens` و`ast` و`asm` و`help`.

## بنية الشجرة

- `src/` — lexer وparser والتحليل الدلالي وcodegen وELF وCLI.
- `src/qessac/qessac.qs` — المترجم المكتوب بلغة Qessa.
- `src/qessac/stdlib.qs` — المكتبة القياسية.
- `examples/` — أمثلة تشغيل اللغة.
- `tests/cli.rs` — اختبارات التكامل.
- `docs/` — التحليل والمواصفات والخطة والمخاطر والتقرير.

## التوثيق

- **المرجع الوحيد:** [المواصفات](docs/spec/MASTER_SPEC.md)، [سجل التحقق](docs/spec/VERIFICATION_LOG.md)، [التتبع](docs/spec/TRACEABILITY.md)


- [التحليل الفني](docs/ANALYSIS.md)
- [المواصفات وحدود الدعم](docs/SPECIFICATION.md)
- [خطة الاختبار ونتائجها](docs/TEST_PLAN.md)
- [خارطة الطريق](docs/ROADMAP.md)
- [المخاطر والقرارات](docs/RISKS_AND_DECISIONS.md)
- [تقرير التنفيذ](docs/IMPLEMENTATION_REPORT.md)
- [خطة التطوير الشاملة وحالة التنفيذ](docs/DEVELOPMENT_PLAN.md)
- [تقييم فجوات الاكتمال والنضج](docs/COMPLETENESS_GAP_ASSESSMENT.md)

المصدر الأصلي: [مشاركة المشروع](https://chat.z.ai/s/c7994bb0-03e6-4fcc-9dab-d79c77915d7b).
