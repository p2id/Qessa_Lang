# خطة الاختبار ونتائج Qessa

## أوامر التحقق

```bash
cargo test --locked
cargo build --release --locked
```

آخر تشغيل لـ `cargo test --locked`: **13 ناجحة، 0 فاشلة**. ينجح بناء release على Linux x86-64، مع 22 تحذيرًا غير قاتل. يمر `rustfmt --check tests/cli.rs` للملف المعدل؛ فحص المشروع الكامل `cargo fmt --check` ما زال يظهر فروق تنسيق قديمة واسعة لم تُنسق ضمن التغيير. سجل التنفيذ الكامل في [IMPLEMENTATION_REPORT.md](IMPLEMENTATION_REPORT.md).

## اختبارات التكامل الموجودة

| الاختبار | ما يتحقق منه |
|---|---|
| `run_forwards_arguments_and_embeds_the_standard_library` | تمرير مسار الملف إلى `cat` و`wc` وتضمين stdlib |
| `read_file_handles_inputs_larger_than_one_megabyte` | قراءة 1,250,000 بايت كاملة دون قص أو تجاوز |
| `run_propagates_the_program_exit_code` | إعادة رمز خروج البرنامج من `qessac run` |
| `function_arguments_beyond_six_follow_the_x86_64_stack_abi_order` | التحقق من ترتيب ومحاذاة وسائط stack في دوال 7 و8 معاملات |
| `later_local_binding_shadows_an_earlier_binding` | أحدث تعريف محلي للاسم نفسه هو المعتمد في lookup |
| `self_hosted_compiler_bootstraps_through_stage_three` | يبني Rust stage-1 ثم stage-2 وstage-3؛ يختبر empty input و7/8 function arguments و`argc/argv`؛ يترجم مصدرًا أكبر من 1 MiB؛ يطابق 8 أمثلة تشغيلًا مع Rust عبر كل المراحل؛ ويؤكد رفض 5 أمثلة غير مدعومة ومدخل malformed دون ELF |
| `compile_emits_a_runnable_elf_executable` | `compile` يخرج ELF قابلًا للتنفيذ مباشرة |
| `parser_recognizes_new_expressions` | تحليل `new Widget()` في AST |
| `unresolved_function_is_reported_before_an_executable_is_written` | رفض الدالة غير المعرفة ومنع إخراج executable |
| `invalid_programs_fail_with_diagnostics_before_emitting_elf` | المتغير المجهول، syscall غير المعروف، غياب `main`، وتكرار `main` |
| `final_example_accepts_semicolons_and_checks_its_claimed_results` | مثال الاختبار النهائي وbreak والنتيجة المشروطة |
| `no_stdlib_mode_runs_standalone_syscall_programs` | تشغيل برنامج syscall مع `--no-stdlib` |
| `all_standalone_examples_execute_successfully` | تشغيل أمثلة arrays/comprehensive/enums/factorial/full_test/hello/modules/pointers/recursion/regions/simple_loop/structs |

## فحص يدوي إضافي

شُغلت جميع ملفات `examples/*.qs` في release. نجحت الأمثلة المستقلة الاثنا عشر؛ أما `cat.qs` و`wc.qs` فكان خروجهما 1 عند تشغيلهما بلا وسيط، وهو سلوك usage المتوقع. اجتازا اختبارات التكامل عند تمرير ملف فعلي. اجتاز `test_final.qs` كل النتائج بما فيها `break: OK`.

## Bootstrap — ناجح ضمن النطاق المختبر

السبب الجذري الذي ثبت: `cg_lookup_local` في المترجم الذاتي كان يبحث من أقدم binding إلى أحدثه. في `parse_program` يعاد استخدام الاسم `n` داخل فرع top-level let ثم لعقدة البرنامج؛ فيقرأ lookup الخانة الأولى بصفر، ما أدى إلى `ast_set_c1(0, funcs)` والكتابة إلى `0x18`. البحث العكسي يطابق `HashMap::insert` في Rust backend ويعيد أحدث تعريف. كما أضيف تحميل معاملات الدوال بعد السادس من stack إلى locals.

اختبار التكامل يبني stage-1، ويستخدمه لإنشاء stage-2، يشغل stage-2 على ملف فارغ، وينشئ stage-3 ويشغله على الملف الفارغ، ثم يثبت أن stage-2 وstage-3 متطابقان byte-for-byte. كذلك يترجم كل من stage-1 وstage-2 وstage-3 برنامجًا ذا سبعة وثمانية معاملات ويشغله بالقيمتين `7` و`708`، بما يغطي odd/even stack arguments. هذا يثبت bootstrap لهذه المسارات المحددة فقط؛ لا يثبت صحة compiler على كل corpus أو اكتمال اللغة.

كما يشغل كل stage ثمانية أمثلة: `arrays`, `factorial`, `full_test`, `hello`, `pointers`, `recursion`, `simple_loop`, و`test_final`، ويقارن stdout وstderr مع Rust CLI. ويختبر رفض `comprehensive`, `enums`, `modules`, `regions`, و`structs` برسالة خطأ ومن دون ترك ملف تنفيذي. هذه المجموعة توثق حد selfhost المعروف ولا تمثل كل نحو اللغة.

يعالج harness أيضًا `argc/argv` بلا وسيطات ومع وسيطين (`alpha`, `beta`) في المراحل الثلاث، ويترجم stage-2 وstage-3 ملفًا ذا تعليق يتجاوز 1,250,000 بايت ثم يشغل الملف الناتج. بذلك تُختبر قراءة المصدر الأكبر من 1 MiB في المترجم الذاتي، وليس فقط `read_file` في برنامج مستخدم.

## معايير التوسعة التالية

التوسعة التالية: اختبار مصادر malformed إضافية مع spans مستقرة، و`argv` بأطوال إضافية وI/O errors؛ إضافة حالات locals متداخلة وstack arguments أكثر من 8؛ وتحديد رسائل رفض أدق بدل parse error عام للميزات غير المدعومة. نجاح corpus الحالية لا يلغي هذه الفجوات.
