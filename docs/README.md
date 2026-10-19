> **إشعار (2026-10-18):** المرجع الوحيد الآن هو [`docs/spec/MASTER_SPEC.md`](spec/MASTER_SPEC.md) وبقية ملفات `docs/spec/`. الوثائق أدناه سجل تاريخي سابق لقرارات المستخدم الأخيرة (لغة هجينة، 4 منصات، ذاكرة تلقائية، أدوات كاملة، لا إصدار قبل اكتمال الرؤية)، وعند التعارض تُعتمد وثائق `spec/`. نتائج الاختبارات المذكورة هنا لم يعد تشغيلها في التدقيق الأخير لغياب Rust، انظر `spec/VERIFICATION_LOG.md`.

# Qessa

Qessa مشروع لغة ومترجم يستهدف إنشاء ملفات ELF مباشرة على Linux x86-64. تتضمن الشجرة الحالية مترجمًا تمهيديًا مكتوبًا بـ Rust، ومترجمًا ذاتي الاستضافة بلغة Qessa، ومكتبة قياسية وأمثلة.

## الحالة الحالية

أُعيد بناء مترجم Rust واختباره. أوامر `compile` و`run` تنتج وتنفذ برامج ELF ضمن النطاق المختبر؛ ونجحت **13 اختبارات تكامل**. اجتاز bootstrap stage-1/2/3؛ وتشغل المراحل الثماني أمثلة المتوافقة مع selfhost بمخرجات مطابقة لـ Rust. ما زالت أمثلة `comprehensive`, `enums`, `modules`, `regions`, و`structs` خارج subset التنفيذ الذاتي وتُرفض دون ELF.

يغطي harness أيضًا `argc/argv` بلا وسيطات ومع وسيطين، ومصدرًا ذاتيًا يتجاوز 1 MiB، ومدخلًا malformed لا ينتج ELF جزئيًا.

**حدود الإثبات:** نجاح bootstrap مثبت ضمن الاختبارات الحالية فقط، ولا يثبت اكتمال اللغة أو جاهزيتها الإنتاجية. راجع [تقرير التنفيذ](IMPLEMENTATION_REPORT.md) و[التحليل الفني](ANALYSIS.md) و[خطة التطوير](DEVELOPMENT_PLAN.md).

## البناء والتشغيل

يتطلب البناء Rust/Cargo، ويُتحقق من التشغيل على Linux x86-64:

```bash
cargo test --locked
cargo build --release --locked
./target/release/qessac run examples/hello.qs
./target/release/qessac run --no-stdlib examples/hello.qs
./target/release/qessac compile examples/hello.qs -o hello
./hello
```

يضمّن `qessac compile` و`qessac run` المكتبة القياسية تلقائيًا؛ استخدم `--no-stdlib` لتعطيل ذلك. وسائط البرنامج تأتي بعد ملف المصدر في `run`، مثل:

```bash
./target/release/qessac run examples/cat.qs input.txt
./target/release/qessac run examples/wc.qs input.txt
```

## ملفات المشروع

| المسار | المحتوى |
|---|---|
| `src/` | lexer وparser والتحليل الدلالي وتوليد x86-64 وELF وواجهة CLI |
| `src/qessac/qessac.qs` | المترجم الذاتي المكتوب بلغة Qessa |
| `src/qessac/stdlib.qs` | المكتبة القياسية المستخدمة في bootstrap |
| `examples/` | أمثلة اللغة واختبارات يدوية |
| `tests/cli.rs` | اختبارات تكامل CLI والبناء والتشغيل |
| `Cargo.toml` و`Cargo.lock` | تعريف حزمة Rust واعتماديات البناء |

## الوثائق

- [التحليل الفني](ANALYSIS.md)
- [مواصفات اللغة وحدودها](SPECIFICATION.md)
- [خارطة الطريق](ROADMAP.md)
- [خطة الاختبارات ونتائجها](TEST_PLAN.md)
- [المخاطر والقرارات المفتوحة](RISKS_AND_DECISIONS.md)
- [تقرير التنفيذ](IMPLEMENTATION_REPORT.md)
- [خطة التطوير الشاملة](DEVELOPMENT_PLAN.md)
- [تقييم فجوات الاكتمال والنضج](COMPLETENESS_GAP_ASSESSMENT.md)

المصدر الذي زوّد به المستخدم: [مشاركة المشروع](https://chat.z.ai/s/c7994bb0-03e6-4fcc-9dab-d79c77915d7b).
