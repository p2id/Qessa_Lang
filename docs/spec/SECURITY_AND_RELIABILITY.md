# الأمن والموثوقية (SECURITY_AND_RELIABILITY)

| الخطر | المسار | الضابط المطلوب |
|---|---|---|
| مدخل عدائي/مشوه | lexer/parser | لا panic ولا حلقة غير منتهية؛ حد عمق التداخل لمنع stack overflow في parser العودي؛ fuzz (AC-09) |
| مصدر ضخم | قراءة الملف | حد قابل للضبط وفشل واضح؛ اختبار >1 MiB موجود وثائقيًا |
| Integer overflow في الأحجام | allocator/الرأس | checked arithmetic (AC-05) |
| نفاد الذاكرة وفشل `mmap` | runtime | فحص النتيجة؛ رسالة موحدة؛ لا استعمال قيمة الخطأ كمؤشر |
| syscall مجهول | codegen | رفض عند الترجمة (QES-SEC-001) |
| silent fallback | codegen/selfhost | `unsupported` خطأ ترجمة (QES-SEC-002) |
| ملف تنفيذي جزئي/مضلل | emitter | كتابة ذرية، حذف عند الفشل (AC-12) |
| ملفات مؤقتة وsymlinks | `qessac run` | أسماء حصرية، `O_EXCL`، عدم اتباع symlink لمسار الإخراج |
| صلاحيات segments | emitter | W^X: لا segment قابل للكتابة والتنفيذ معًا |
| ثقة المترجم الذاتي (trusting trust) | bootstrap | seed موثق، بناءان مستقلان وchecksums (AC-07)، وثيقة provenance |
| سلسلة التوريد | Cargo/artifacts | `Cargo.lock`، `cargo deny/audit`، SBOM، checksums (AC-19, AC-20) |
| تنفيذ برنامج المستخدم في `run` | CLI | توثيق أنه يشغّل الكود الناتج بصلاحيات المستخدم؛ لا sandbox ضمنيًا |
| مدير الحزم | تنزيل/تنفيذ | تحقق checksum/توقيع، عدم تنفيذ سكربتات تلقائيًا |
