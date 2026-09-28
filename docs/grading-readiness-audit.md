# تقرير تدقيق الجاهزية والجودة — محرك التسوية (Grading Engine)

**النطاق:** `crates/landsurvey/src/` (المحرك) + `src/dispatch.rs` / `src/interactive.rs` (طبقة الربط مع الهوست).
**المصدر الوحيد للحقيقة:** مخرجات `cargo check --workspace` و`cargo test -p landsurvey` المنفذة الآن + قراءة سطرية للكود. كل ادعاء مسند بملف:سطر.

---

## 1. صحة التجميع والمواءمة مع Host API (سياسة صفر تحذيرات)

### 1.1 خط الأساس: `cargo check --workspace`

| الحزمة | النتيجة |
|---|---|
| `landsurvey` (المحرك) | ✅ يترجم — **15 تحذيرًا**، صفر أخطاء |
| `landsurvey-cli` | ✅ يترجم (فحص ضمن الـworkspace، لا تحذيرات جديدة) |
| `opencad-landsurvey-plugin` (cdylib) | ❌ **7 أخطاء + 4 تحذيرات — لا يترجم أصلًا** |

**تحذيرات المحرك الـ15** (كلها في `featureline/`، ولا تحذير واحد في `surface.rs` أو `landxml.rs`):

- `featureline/entity.rs:7` — `unused import HashMap`
- `featureline/breakline.rs:6` — `unused imports Node, Tri`
- `featureline/breakline.rs:7` — `unused imports FeatureLineSyncReport, FeatureVertex, Point3d, ZSource`
- `featureline/breakline.rs:94` — متغير ميت `new_idx`
- `featureline/breakline.rs:120-122` — `a, b, c` ميتة
- `featureline/breakline.rs:255` — `tris` ميتة
- `featureline/breakline.rs:319` — `d` ميتة
- `featureline/breakline.rs:377` — معامل `edge_to_triangles` غير مستخدم
- `featureline/breakline.rs:384-385` — `tri1, tri2` ميتتان
- `featureline/breakline.rs:408-409` — `orient_acd, orient_bdc` ميتتان
- `featureline/breakline.rs:412` — `old_edges` ميتة

**أخطاء الـcdylib الـ7** (موجودة قبل أي كود تسوية، وتمنع تسجيل أي أمر grading جديد حتى إصلاحها):

1. `src/interactive.rs:200` — `E0407: method on_finish is not a member of trait InteractiveCommand`
2. `src/interactive.rs:217` — `E0603: function draw_tin is private` (معرّفة في `src/dispatch.rs:1276` بدون `pub`)
3. `src/dispatch.rs:1214` — `E0599: no method get … impl Iterator<Item=&EntityType>` (كود `find_boundary_polygon`)
4. `src/dispatch.rs:1236` — `E0599: no method vertices … field, not a method` (كود `lwpoly_to_polygon`)
5. `src/dispatch.rs:1241` — `E0599: … entities().get(handle)` (كود `find_entity_by_handle`)
6. `src/interactive.rs:203` — `E0599: no method add_breakline … for Surface` (القضية 5 أدناه — خطيرة)
7. `src/interactive.rs:242` — `E0599: no variant Finish … for CommandStep` (الكود يستخدم `NeedPoint/Cancel/CommitAndEnd` في `interactive.rs:93,97,193`، لكن `Finish` غير موجود في التريت الحالي)

**تحذيرات الـcdylib الـ4:** `interactive.rs:6` (`Handle` غير مستخدم)، `interactive.rs:10` (أربعة imports ميتة: `surface_tag, find_surface_in_document, first_arg, place`)، `interactive.rs:11` (`chrono`)، `interactive.rs:185` (`tin_z` ميت).

### 1.2 `cargo test -p landsurvey` — أخضر بالكامل

```text
38 passed (lib) + 5 passed (road_surface_volume_golden) + 1 passed (volume_pnezd) + 0 doc — 0 failed
```

لا تحذيرات تشير إلى `surface.rs` أو `landxml.rs` في مخرجات الاختبار.

### الحكم على القسم 1: ❌ سياسة صفر التحذيرات **غير متحققة**

المحرك وحده قريب (15 تحذيرًا محصورة في `featureline/`)، لكن طبقة الربط **مكسورة التجميع** — أي أمر `LS_GRADING_*` جديد لا يمكن بناؤه أو اختباره عبر الهوست قبل إصلاح البنود 3–7 أعلاه. الدين التقني في `dispatch.rs` المؤثر على التسوية: `find_boundary_polygon` و`find_entity_by_handle` يستخدمان API كيانات لم يعد موجودًا (`entities().get` + `lw.vertices()` كدالة)، فيلزم أولًا اكتشاف الـAPI الصحيح من `acadrust`/`ocs_plugin_api` وتثبيته قبل أن تعتمد عليه أوامر التسوية في قراءة البوليلينات.

---

## 2. القضايا السبع

### القضية 1 — المنحنيات والـbulge: ❌ غير جاهز (مانع P0)

- التمثيل موجود فقط: `featureline/entity.rs:41-54` — `FeatureVertex { pt: Point3d, bulge: f64 /* tan(sweep/4) */, … }`، و`plan.rs:129-145` (`bulged_segments`) **يسرد** الأقواس فقط لمطابقة التكرارات (`arc_duplicates_segment` في `plan.rs:573-608`)، لا يقطّعها.
- بحث شامل (`bulge|tessellat|densif|sagitta|chord|sample_along|step_sample`) أثبت: **لا توجد أي دالة tessellation/densification ولا معيار chord-tolerance/sagitta في المحرك**. ولا توجد مماسات/نواظم لحظية للمنحنيات.
- النتيجة: أي إسقاط ميل (projection) من `FeatureLine` منحنية سيُطلق الأشعة من نقاط الرؤوس فقط ويتجاهل القوس — خطأ هندسي مباشر في اتجاه وموضع الميل.
- **المطلوب قبل الإنتاج:** دالة `densify_bulge(p1, p2, bulge, sagitta_tol) -> Vec<Point3d>` + مماس/ناظم لحظي لكل عينة، في `grading/math.rs` الجديد (مع test وحدوي: قوس 90°، قوس صغير، bulge=0 يمر شفافًا).

### القضية 2 — استعلام الارتفاع وأشعة النهار: ⚠️ صحيح لكن غير قابل للتوسع (مانع P0 للأداء)

- `Surface::interpolate_z` (`surface.rs:210-225`) و`find_containing_triangle` (`surface.rs:229-244`): استيفاء باريسنتري **دقيق على مستوى الفاسيت** (`wa·za+wb·zb+wc·zc`)، يتجاوز المثلثات المنحلة (`d.abs() < EPS`)، وتسامح حافة `-1e-9`. رياضيًا سليم.
- **التعقيد `O(T)` مسح خطي لكامل المثلثات في كل استعلام** — لا فهرس مكاني من أي نوع. سطح الاختبار الذهبي 3234 مثلثًا؛ تتبع شعاع نهار واحد بعشرات الخطوات الفرعية × مئات الأشعة = عشرات ملايين اختبارات مثلثات. هذا تدهور حاد متوقع (تعليق الاستجابة)، وليس مجرد بطء.
- السلوك خارج الحدود **آمن**: يُرجع `None` (لا panic) — والمسار الحرج `interactive.rs:154-159` يعالج `None` بإعادة طلب النقطة. وهذا هو المسار المتعاقد عليه لأشعة النهار التي تخرج دون لقاء (Out-of-bounds): `None` → إنهاء آمن للشعاع، لا انهيار.
- ملاحظة دقيقة: مسار الحجوم يستخدم عمدًا `plane_z` الاستقرائي (`surface.rs:1060`) بدل استعلام الاحتواء — وهو الصحيح للرؤوس المتداخلة حتى ضجيج التقريب، لكنه **غير صالح** كبديل لاستعلامات النهار (قد يستقرئ خارج السطح). يجب عدم الخلط بينهما.
- **المطلوب:** فهرس مكاني (uniform-grid أو bbox-grid على المثلثات، std فقط) + اختبار أداء يقيس زمن 10k استعلام على سطح 3234 مثلثًا قبل/بعد.

### القضية 3 — الأركان (مروحة محدبة / منصف مقعر): ❌ مفقود بالكامل (مانع P0)

- بحث (`fan|miter|bisect|offset_poly|self_intersect|bowtie|daylight|grading`) في `crates/landsurvey/src/`: **صفر** — لا مروحة شعاعية، لا منصف زاوية، لا offsetting، لا قص تقاطعات ذاتية. `cogo.rs` كله (84 سطرًا) = `inverse` + `azimuth_to_bearing` فقط.
- ما وُجد عن "grading" هو **خطط ورقية فقط**: `FEATURELINE_PARITY_PLAN/PHASE4_AdvancedGrading_TechPlan.md:257-270` (تصميم `generate_daylight_line`/`create_grading_surface`) و`:321-360` (تصميم `grading.rs`/`GradingObject`) — لا كود منتج يقابلها.
- **المطلوب:** `grading/corners.rs`: مروحة مخروطية للأركان الخارجية المحدبة (توليد أشعة بزاوية خطوة + شروط عدم فراغ)، ومنصف زاوية + كشف/قص bowtie للأركان الداخلية المقعرة، باختبارات: زاوية حادة/منفرجة/قائمة في الاتجاهين.

### القضية 4 — تثليث المنصة الداخلية (Infill): ⚠️ نصف جاهز (P1)

- اللبنات موجودة وتعمل: `from_points` → `delaunay` Bowyer–Watson (`surface.rs:109`, `:644`) + `apply_outer_boundary` (`surface.rs:336-346`) الذي يُسقط خارج المضلع **بمعيار centroid**.
- **الثغرة الدقيقة:** معيار الـcentroid يحتفظ بالمثلث كاملًا أو يسقطه كاملًا — مثلثات **تعبر حد المنصة** تُحسم بالكامل لجهة واحدة، فحد الـpad مسنن بدقة مثلث واحد، لا قص دقيق على الحافة. للتسوية الدقيقة يلزم قص exact على المضلع (نفس أدوات `clip_polygon` المستعملة في الحجوم) أو CDT مقيد فعلي.
- إيجابي: `outer_boundary` مخزن ومتسلسل (`serde(default)`) وقابل للفحص — صالح كآلية حماية مبدئية للـinfill.

### القضية 5 — خطوط الانكسار وخط النهار: ❌ غير جاهز (مانع P0 — كسر تجميعي + خوارزمي)

- **كسر تجميعي:** `interactive.rs:203` يستدعي `self.surface.add_breakline(...)` — ولا توجد **أي** دالة بهذا الاسم على `Surface` (الموجود دالة حرة `breakline::add_breakline(surface, …)` في `breakline.rs:29-33`). هذا أحد أخطاء الـ7 التي تمنع بناء الـcdylib.
- **تجاهل النوع:** المعامل `_breakline_type` (`breakline.rs:32`) غير مستخدم — الأنواع الستة (`entity.rs:260`) سلوكها متطابق، بما فيها أي نوع مستقبلي لخط النهار.
- **إدراج هش:** مطابقة 1مم (`round_coord`, `breakline.rs:137-139`) ستتصادم مع تقاطعات نهار بمستوى الميكرون (اصطدامات/اندماجات خاطئة)؛ النقطة على الحافة **لا تُشق** بل تُترك يتيمة (`breakline.rs:111-116` تعليق "for simplicity… let constraint recovery handle it")؛ الاسترداد edge-flip فقط بسقف صامت `max_iterations = T*10` (`breakline.rs:167,222-225`) قد يتوقف دون اكتمال؛ `flip_edge` يحتوي `unwrap()`ين (`breakline.rs:318-319`) قابلين للـpanic على مدخلات منحلة.
- **صفر اختبارات:** لا `#[cfg(test)]` في أي ملف تحت `featureline/` (مؤكد بالبحث).
- **المطلوب:** `Surface::add_breakline` حقيقية (أو إصلاح الاستدعاء)، تفعيل النوع، snap متكيف (لا 1مم ثابت)، شق حافة 2-to-4، إزالة الـunwrap القابل للوصول، أول حزمة اختبارات breakline (مستقيم/متقاطع/على حافة/خارج الهيكل).

### القضية 6 — دمج الأسطح (Pasting): ❌ مفقود (قرار معماري P1)

- بحث `paste_surface`: **صفر نتائج** في كامل المستودع. لا قص للـEG تحت المنصة ولا لحام (stitch) على خط النهار.
- المتاح اليوم: حجوم مستقلة فقط عبر `exact_composite_cut_fill` (`surface.rs:933`) — كافٍ **للمقارنة الكمية** (تصميم vs طبيعة) لكنه ليس سطحًا نهائيًا مدمجًا.
- **المطلوب قرار صريح قبل الترميز:** إما (أ) `grading/paste.rs` حقيقي (قص EG داخل Daylight + لحام شبكة التسوية — عمل طوبولوجي كبير)، أو (ب) تجميد النطاق رسميًا على "سطح تسوية مستقل + تقرير حجم"، مع توثيق ذلك كقيد منتج.

### القضية 7 — النظافة المعمارية ومعالجة الأخطاء: ⚠️ (P1 — إصلاح سريع لكن إجباري)

- **الأخطاء:** كل أخطاء المحرك نصوص سائبة — `Result<_, String>` (`surface.rs:336,350`، `breakline.rs:33,60,92,142,243,378`) و`&'static str` (`transform.rs:125`, `resection.rs:84`)، ولا `LandSurveyError`/`GradingError` منظمة في أي مكان. لا `panic!` صريح في المحرك، لكن `unwrap()` الإنتاجية في مسارات قابلة للوصول: `plan.rs:243,421` (`arr.last().unwrap()`)، `breakline.rs:318-319` (الأخطر). الباقي (`landxml` writeln، اختبارات) حميد.
- **صفر unsafe:** ✅ مؤكد — بحث `unsafe` في `crates/landsurvey/src/` أعاد **صفر نتائج** (الضجيج الوحيد `atomic.rs` في ريجيستري خارجي ظهر في رسالة مساعدة المترجم فقط).
- **الاستنساخات:** خارج الاختبارات، الاستنساخات محدودة ومبررة (`from_points:113 points.to_vec()`، `340,354 polygon.to_vec()` للتخزين، `delaunay:663 verts`، `clip_polygon:1009 subject.to_vec()`). الحلقة الساخنة (`interpolate_z`) بلا تخصيص heap لكل مثلث (مصفوفات ستاك) — التكلفة الحقيقية هي الـ`O(T)` نفسه لا الاستنساخ.
- **المطلوب:** تنظيف الـ15 تحذيرًا (معظمها حذف imports/متغيرات ميتة — عمل روتيني)، نوع `GradingError` منظم، وإزالة/حراسة `unwrap` القابل للوصول في `breakline.rs:318-319` و`plan.rs:243,421`.

---

## 3. الحكم النهائي

### 🟢 إشارات خضراء (يبني عليها التسوية مباشرة)

1. **الحجوم الذهبية مستقرة:** 5/5 مطابقة Civil 3D 2026 (بما فيها terrain-vs-terrain بعد إصلاح `plane_z`) — `exact_composite_cut_fill` و`volume_to_datum` جاهزان لتقارير الكميات.
2. **`interpolate_z` / `find_containing_triangle`:** دقيقان تعاقديًا وآمنا الحدود (`None`) — يصلحان كمرجع صحة (oracle) للفهرس المكاني القادم.
3. **الحدود (`outer/hide`)** والتسلسل والـContours والـLandXML: تعمل ومختبرة — تصلح لحماية الـpad وتبادل الأسطح.
4. **صفر `unsafe`** في المحرك؛ لا تخصيصات مهدرة في الحلقات الساخنة.

### 🔴 المتطلبات المسبقة والمعوقات (مرتبة)

| الأولوية | البند | الدليل |
|---|---|---|
| **P0 — يمنع البناء** | إصلاح أخطاء الـcdylib الـ7 (خاصة `add_breakline` المفقودة، `CommandStep::Finish`، `entities().get`، `vertices`، `on_finish`، `draw_tin` الخاصة) | `cargo check --workspace` أعلاه |
| **P0** | tessellation الأقواس (bulge→قطع + مماس/ناظم) | لا وجود — القضية 1 |
| **P0** | فهرس مكاني للاستعلام + حد أداء مُقاس | `O(T)` في `surface.rs:210-244` — القضية 2 |
| **P0** | مروحة الأركان + منصف + قص bowtie | لا وجود — القضية 3 |
| **P0** | إصلاح `add_breakline` (API + نوع + snap + شق + unwrap + اختبارات) | `breakline.rs:29-139,318-319` + صفر `#[cfg(test)]` — القضية 5 |
| **P1** | تنظيف 15+4 تحذيرًا + `GradingError` + حراسة unwrap | القضية 7 |
| **P1** | قرار Paste مقابل سطح مستقل + (إن لزم) `paste.rs` | لا `paste_surface` — القضية 6 |
| **P1** | قص exact لحد الـpad بدل centroid فقط | `surface.rs:336-346` — القضية 4 |

### 🏗️ المعمارية المقترحة للتسوية (ملتزمة بمعايير المحرك: `std` فقط، بلا `unsafe`)

```text
crates/landsurvey/src/grading/
├── mod.rs        # إعادة التصدير + GradingConfig المشتركة
├── error.rs      # GradingError المنظم (يحل محل Result<_, String> في هذا المجال)
├── math.rs       # أساسيات: densify_bulge، مماس/ناظم، ray-segment، إسقاط، مساحة/اتجاه
├── criteria.rs   # معايير الميل (نسبة/زاوية، cut/fill، مسافة بحث قصوى)
├── project.rs    # إسقاط FeatureLine (مكثّفة) على EG بخطوات فرعية + حد None الآمن
├── corners.rs    # مروحة محدبة / منصف مقعر / قص bowtie
├── daylight.rs   # تتبع الأشعة + خط النهار كـ FeatureLine
├── infill.rs     # تثليث الـpad + حماية outer_boundary (+ قص exact لاحقًا)
└── paste.rs      # (اختياري، خلف قرار النطاق) دمج التصميم على EG
```

**بوابات القبول قبل أي كود إنتاج:** (1) الـcdylib يترجم بلا أخطاء؛ (2) صفر تحذيرات في `featureline/` و`grading/`؛ (3) اختبارات وحدوية لكل وحدة أعلاه بما فيها حالات الحافة (قوس صفري، شعاع يخرج دون لقاء، زاوية 180°/حادّة، daylight على حافة مثلث، pad غير محدب)؛ (4) مقياس أداء للاستعلام المكاني على سطح 3234 مثلثًا؛ (5) قرار Paste موثق.

**الخلاصة:** الأساس العددي (حجوم/استعلام/حدود) صلب ومُثبت ذهبيًا، لكن **التسوية لا تُبنى بعد** — خمسة معوقات P0 (تجميع مكسور، tessellation، فهرس مكاني، أركان، breakline) يجب إغلاقها أولًا.
