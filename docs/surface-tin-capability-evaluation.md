# أسئلة تقييم التعديلات المنجزة على قدرات سطح TIN

وثيقة إجابة فنية عن الاستمارة (13 قسمًا). كل إجابة مسندة إلى **اسم الملف + رقم السطر / اسم الدالة** في المستودع، أو موسومة صراحةً بـ **«غير منجز»** عند عدم وجود ما يدعمها.

**منهجية:** قراءة مباشرة للمصدر + تشغيل فعلي لـ `cargo test -p landsurvey` + بحث نصي (grep) عن كل ميزة مذكورة.

---

## 1) نموذج بيانات السطح (TIN Data Model)

### 1.1 بنية `Surface` الأساسية — **منجز (مختصر)**
```rust
// crates/landsurvey/src/surface.rs:24
pub type Node = [f64; 3];   // [x, y, z]
// crates/landsurvey/src/surface.rs:26
pub type Tri  = [usize; 3]; // [i, j, k]
// crates/landsurvey/src/surface.rs:64-67
pub struct Surface {
    pub nodes: Vec<Node>,
    pub triangles: Vec<Tri>,
}
```
- النموذج مسطّح (flat arrays) وحقوله `pub` أي قابلة للتعديل من الخارج مباشرةً.
- **لا** يحتوي: حدود سطح (boundary loop)، بيانات مصدر لكل نقطة (source/desc)، مرجع خطوط مرتكزة، أو أي ميتاداتا (وحدات/CRS).

### 1.2 التخزين والاسترجاع — **منجز جزئيًا**
- تخزين أثناء الجلسة: حالة العملية المحلية `LandSurveyState { surfaces: Vec<StoredSurface>, ... }` — `src/state.rs:36-44`، والإضافة/الاستبدال بالاسم `put_surface` — `src/state.rs:49-62`، والبحث `get_surface` — `src/state.rs:65-70`.
- الاسترجاع من الرسم: عبر وسم الكيان `surface_tag(e)` — `src/dispatch.rs:857` وإعادة بناء السطح `find_surface_in_document(...)` — `src/dispatch.rs:875` (موقع الاستدعاء في `featureline_create`: 1472).
- **لا يوجد** تسلسل (serialization) للسطح نفسه في المحرك: `Surface` يشتق من `Debug, Clone, PartialEq` فقط — `crates/landsurvey/src/surface.rs:63` (لا `Serialize`). الاستمرارية تعتمد كليًا على كيانات المhost وXDATA.

### 1.3 سجل العمليات / تاريخ التعديلات (Operation Log) — **غير منجز**
- لا يوجد أي سجل داخلية داخل `Surface`.
- الموجود هو تراجع على مستوى الرسم فقط: `host.push_undo("LS_SURFACE")` — `src/dispatch.rs:535`، `push_undo("LS_VOLUME draw")` — `src/dispatch.rs:784`، `push_undo("LS_FEATURELINE_CREATE")` — `src/dispatch.rs:1553` (تراجع عن رسم الكيانات، لا عن تعديل بيانات السطح).
- دليل ضمني على الفجوة: تعليق المرجع في `remove_breakline` يعترف أن الإزالة الكاملة تتطلب «تخزين التحويج الأصلي» — `crates/landsurvey/src/surface.rs:206`.

### 1.4 معلومات الترابط المجاور (Adjacency) — **غير منجز**
- لا توجد بنية half-edge / wing-edge / DCEL مخزّنة.
- الترابط يُعاد حسابه عند كل استدعاء:
  - الحواف الفريدة `edges()` عبر `HashSet` محلي — `crates/landsurvey/src/surface.rs:91-103`.
  - خريطة `edge_to_triangles` تُبنى من الصفر في كل إضافة breakline — `crates/landsurvey/src/featureline/breakline.rs:104-115`.
  - خريطة `vertex_to_tris` تُبنى من الصفر لحساب المنطقة المتأثرة — `crates/landsurvey/src/featureline/breakline.rs:316-322`.

---

## 2) خوارزمية التثليث

### 2.1 الخوارزمية الأساسية — **منجز (Bowyer–Watson)**
- قسم موثّق في الكود: `// --- Delaunay triangulation (Bowyer-Watson) ---` — `crates/landsurvey/src/surface.rs:567`.
- الدالة الرئيسية `delaunay(points)` — `crates/landsurvey/src/surface.rs:589-657`:
  - `n < 3` تُرجع مصفوفة فارغة — السطر 591-593.
  - super-triangle حول المدى — 606-612، وإزالته لاحقًا — 644-648.
  - اختبار الدائرة `in_circumcircle` — 574-585 بعتبة عددية `det > 1e-12` — السطر 584.
  - اختبار الاتجاه `orient2d` — 570-572، وتهذيب الاتجاه CCW — 650-654.

### 2.2 بناء السطح من نقاط / التثليث المقيد — **منجز + مقيد جزئيًا**
- البناء العادي غير مقيد: `Surface::from_points` — `crates/landsurvey/src/surface.rs:71-77` (يمرر XY فقط إلى `delaunay`، وZ كما هو).
- **لا** يوجد تقييد حدود خارجي (boundary constraint) عند البناء.
- التثليث المقيد (constrained) موجود فقط عبر مسار الـ breakline:
  - `Surface::add_breakline` — `crates/landsurvey/src/surface.rs:201-203` يستدعي `featureline::breakline::add_breakline` — `crates/landsurvey/src/featureline/breakline.rs:29-58`.
  - الخطوات: إدراج عقد (41-42 ← `insert_breakline_vertices` 61-88)، توليد حواف مقيدة (44-46)، ثم `retriangulate_with_constraints` (51 ← تعريفها 96-186) وهي **edge-flip تدريجي** تحمي الحواف المقيدة (139-142) وتتوقف عند حد أمان `max_iterations = triangles.len() * 10` (124، 180-182).

### 2.3 النقاط المكررة / المتقاربة / المستقيمة — **غير مُعالَجة (فجوة)**
- **لا يوجد إزالة تكرار** لا في `from_points` (السطر 74 ينسخ كل النقاط) ولا في حلقة `delaunay` (614-642).
- الحماية موجودة فقط عند *الاستهلاك*:
  - التدوين الخطي يتجاوز المثلثات المُفلَتة `d.abs() < EPS` — `interpolate_z` 166-167 و`find_containing_triangle` 185-186، مع `const EPS: f64 = 1e-12` — السطر 28.
  - عتبة `1e-12` في `in_circumcircle` — السطر 584.
- النقاط المستقيمة (collinear): مثلثات بمساحة صفرية تبقى كما هي (لا يُفرَض فيها تذبذب رمزي) — 650-654.
- **لا** يوجد dedup عند إدراج عقد الـ breakline إلا بتمييز إحداثيات مقرَّبة لـ 1 مم `round_coord` — `breakline.rs:90-93`.

### 2.4 التعقيد الحسابي — **O(n²) عمليًا، بلا تحليل/قياس**
- لكل نقطة: مسح **كامل** لكل المثلثات لاختبار الدائرة — `surface.rs:616-620`، ثم `edge_count.iter().find` خطّي داخل الحلقة — 627. النتيجة: مسحات O(n²) على الأقل لكل إدراج.
- الاستعلامات موضعية خطية أيضًا: `interpolate_z` تمسح كل المثلثات — 163؛ `find_containing_triangle` — 182.
- التراكب بين سطحين موثّق صراحةً `O(Nt*Nb)` — تعليق المرجع `surface.rs:349`.
- **لا يوجد** benchmark/criterion في المستودع (بحث: لا يوجد `benches/`؛ التخطيط فقط في `FEATURELINE_PARITY_PLAN/README.md:126` و`PHASE5_ProductionPolish_TechPlan.md:555`).

### 2.5 الحالات المُسطّحة (majority voting / تذبذب رمزي) — **غير منجز**
- لا يوجد symbolic perturbation أو majority voting في أي مكان؛ التعامل كله عددي عبر العتبات (`EPS = 1e-12` surface.rs:28، `det > 1e-12` surface.rs:584، `wa >= -1e-9` surface.rs:172).

---

## 3) Breaklines

### ما الذي نُفِّذ
| القدرة | الحالة | الدليل |
|---|---|---|
| إضافة breakline إلى سطح | منجز في المحرك | `Surface::add_breakline` — `surface.rs:201-203` ← `breakline.rs:29-58` |
| 6 أنواع breakline | منجز | `BreaklineType { Standard, Wall, RetainingWall, Curb, Gutter, FlowLine }` — `entity.rs:260-267` |
| نتائج/مقاييس الإضافة | منجز | `BreaklineResult { vertices_added, triangles_modified/added/removed, affected_area }` — `entity.rs:271-277` |
| إدراج عقد مع تفادي التكرار (1 مم) | منجز | `insert_breakline_vertices` — `breakline.rs:61-88` |
| تثليث مقيد (حماية الحواف) | منجز (بتحفّظ) | `retriangulate_with_constraints` — `breakline.rs:96-186`، حماية الحواف المقيدة 139-142 |
| إزالة breakline | منجز تقريبيًا | `remove_breakline` تعليق صريح: تقريب يتطلب تخزين التحويج الأصلي — `surface.rs:205-213` |
| المنطقة المتأثرة | منجز | `calculate_affected_area` — `breakline.rs:315-335`، `find_affected_triangles` (فلترة bbox) — `surface.rs:229-251` |
| مزامنة feature lines مع السطح | منجز | `sync_linked_feature_lines` — `surface.rs:217-227`، `FeatureLine::sync_surfaces` — `entity.rs:406` |
| اختبارات آلية | **غير منجز** | صفر `#[cfg(test)]` في `featureline/*.rs` |

### فجوتان أساسيتان يجب تقريرهما

**أ) فجوة توصيل (لا يوجد أي مستهلك للـ breakline):**
بحث نصي في مجلد `src/` كله: **لا يوجد استدعاء واحد لـ `add_breakline`**. مسارا إنشاء الخط الهندسي ينتهيان برسم `LwPolyline` عادي فقط:
- المسار التفاعلي ينهي بأمر `CommitAndEnd(entity)` من `LwPolyline` فقط — `src/interactive.rs:191-193`، مع `build_xdata` موجودة (111-119) لكنها **غير مستدعية** أبدًا.
- مسار CSV يبني `FeatureLine` ثم يحوّلها إلى `LwPolyline` + سجل XDATA بقيم Z فقط — `src/dispatch.rs:1546-1563`.

⇒ ميزة constrained Delaunay «محرك جاهز وغير موصول» من الواجهة.

**ب) فجوة خوارزمية (العقد الجديدة قد لا تُربط بالتحويج):**
`retriangulate_with_constraints` — `breakline.rs:96-186` تعمل **flip فقط بين المثلثات الموجودة** (127-183). لا توجد خطوة Bowyer–Watson لإدراج النقاط الجديدة، ولا إنشاء للمثلثات التي تربط فقرات breakline الجديدة. عقد breakline المضافة في السطر 79-80 قد تبقى معزولة، والحواف المقيدة المُعلَنة في 44-46 لا تُنشأ فعليًا إن لم تكن حواف مثلثات قائمة أصلًا. يُضاف إلى ذلك حد الأمان (180-182) الذي قد ينهي الحلقة دون اكتمال، واستخدام `_breakline_type` غير مستخدم — `breakline.rs:32` (النوع لا يؤثر على السلوك)، واختبار `is_delaunay` محلي بعتبة `1e-12` — `breakline.rs:190-221` مغاير جزئيًا لـ `in_circumcircle` الأساسي.

---

## 4) تعديل السطح

### 4.1 إضافة/حذف نقاط أو مثلثات — **غير منجز**
لا توجد دالة `add_point` / `remove_point` / `remove_triangle`. الحقول `pub` (`surface.rs:65-66`) تسمح بالتعديل اليدوي برمجيًا فقط، بلا أي API أو أمر.

### 4.2 تعديل ارتفاع نقطة / إعادة تثليث جزئي — **منجز جزئيًا وغير موصول**
- المتاح: إعادة بناء كاملة (`Surface::from_points` — 71-77)، وإضافة/إزالة breakline (§3) — وكلاهما خارج أي أمر تعديل.

### 4.3 أوامر تعديل السطح — **غير منجز**
- سجل الأوامر كاملًا 14 أمرًا فقط: `LS_HELLO, LS_PNEZD, LS_AUTOLABEL, LS_IMPORTPLAN, LS_INVERSE, LS_VOLUME, LS_SURFACE, LS_LANDXML|LANDXMLIMPORT, LS_DATUM, LS_RTS, LS_HELMERT, LS_RESECT, LS_LIST, LS_FEATURELINE` — `src/dispatch.rs:57-126`.
- لا يوجد أمر تعديل/حذف/دمج/قص. الاستبدال الوحيد: إعادة `LS_SURFACE` واستبدال السطح بالكامل بالاسم — `put_surface` `src/state.rs:49-62` المستدعى من `src/dispatch.rs:544`.

---

## 5) مصادر بيانات متعددة

### 5.1 صيغ الإدخال المدعومة — **منجز (3 صيغ)**
- PNEZD csv/txt — زر `LS_PNEZD` (قائمة ملفات csv/txt) — `src/ribbon.rs:27-37`.
- خطة JSON (plan2cad/plat2json) — `LS_IMPORTPLAN` — `src/ribbon.rs:48-58`.
- LandXML xml/landxml — منتقيا `LS_SURFACE` (csv/txt/xml/landxml) — `src/ribbon.rs:107-112` و`LS_LANDXML` — 117-127.
- غير مدعوم: DEM، ASCII grid، XYZ بترميزات أخرى.

### 5.2 لصق/دمج سطحين (Paste Surface) — **غير منجز**
بحث `paste` في `.rs`: نتائج لا علاقة لها (رسائل SVG explainer فقط — `src/dispatch.rs:428,1088,1214`). لا توجد دالة دمج. الموجود هو *مقارنة* لا دمج: `volume_between_shared` تتطلب طوبولوجيا متطابقة — `surface.rs:275-289`.

### 5.3 التحويل بين المصادر (وحدات/إحداثيات) — **منجز جزئيًا للنقاط فقط**
- تحويل نقاط: `LS_RTS` / `LS_HELMERT` (وحدة `transform.rs`) — لكنهما يعملان على نقاط PNEZD وليس على أسطح.
- LandXML: الوحدات «imported as-is» بلا تحويل ولا تخزين — `src/dispatch.rs:554-555`.

### 5.4 استيراد DEM (شبكة نقطية/رستر) — **غير منجز**
بحث `DEM|ascii` في `*.rs`: صفر نتائج.

---

## 6) التحليلات

| البند | الحالة | الدليل |
|---|---|---|
| كنتورات ارتفاعية عامة (contour بأي ارتفاع) | **غير منجز** | الموجود «كنتور» محصور في حدود الحجم فقط: `cut_fill_to_datum_detailed` يعيد مقطع `z = datum` — `surface.rs:141-158`، وحدّ `dz = 0` بين سطحين — `surface.rs:334-341`. لا يوجد أمر `LS_CONTOUR`. |
| تحليل الانحدار/الاتجاه (slope/aspect) | **غير منجز** | بحث `slope|aspect`: صفر نتائج. حقول `grade_in/grade_out` حقلي بيانات فقط — `entity.rs:51-53`، بلا حساب. |
| إحصاءات السطح | **منجز جزئيًا** | المخرجات: عدد النقاط/المثلثات/الحواف/المساحةالمستوية — `src/dispatch.rs:540-549` و`crates/landsurvey-cli/src/main.rs:82-88`. **غائب:** min/max/متوسط Z، المساحة الحقيقية (3D)، متوسط الانحدار، كثافة النقاط. |
| cut/fill ضد مستوى أفقي | منجز | `volume_to_datum` — `surface.rs:122-131`، `cut_fill_to_datum(_detailed)` — 135-158. |
| التصور (رسم TIN/حدّ حجم/تسميات) | منجز جزئيًا | `draw_tin` / `draw_segments` / `draw_volume_label` — `src/dispatch.rs:783-794`؛ وحد `viz.rs`. مرتبط بأوامر الرسم لا كتحليل مستقل. |
| تصدير تقارير تحليلية (CSV/JSON) | **غير منجز** | الإخراج نصوص `push_output` فقط؛ ملفات SVG explainer لـ inverse/rts/helmert — `src/dispatch.rs:428,1088,1214`. |

---

## 7) الحجوم (Volumes)

| السؤال | الحالة | الدليل |
|---|---|---|
| 7.1 حجم داخل منطقة/حدود محددة | **غير منجز** | كل الدوال تحتوي **كل** المثلثات: `volume_to_datum` — `surface.rs:122-131`، `exact_composite_cut_fill` — 351-353. لا معامل قص/حدود. |
| 7.2 حجم بين سطحين (top − bottom) | **منجز** | TIN overlay دقيق: `composite_cut_fill_detailed` / `exact_composite_cut_fill` — `surface.rs:344-353` (قص كل facet ضد كل facet، تقسيم عند `dz=0`، تكامل دقيق، موثّق `O(Nt*Nb)` — السطر 349)؛ وطريقة الطوبولوجياالمتطابقة `volume_between_shared` — 275-289. |
| 7.3 حجم قطع/تعبئة ضد مستوى datum | **منجز** | `cut_fill_to_datum` يقسم المثلثات المتقاطعة عند مقطع datum تمامًا — `surface.rs:133-158`. |
| 7.4 تقرير/تصدير النتائج | **منجز جزئيًا** | نصوص + تسمية على الرسم — `src/dispatch.rs:761-772`، وCLI يكتب DXF (طوبولوجيا + حدّ + تسمية) — `cli/src/main.rs:105-153`. لا يوجد تقرير CSV/JSON منفصل. |
| 7.5 الطريقة الشبكية (grid method) | **منجز** | `grid_cut_fill` — `surface.rs:291-332` (عيّنة مركز خلية للسطحين، `cut` موجب، «الدقة تعتمد على grid_step» — السطر 296). خطوة الشبكة **قابلة للضبط** من المستخدم: `LS_VOLUME [<top> <bottom>] [grid_step] [draw]` — `src/dispatch.rs:706-722`، و`--grid step` في CLI — `cli/src/main.rs:115-126`. |

---

## 8) واجهة سطر الأوامر (CLI)

### 8.1 إحصاءات السطح من CLI — **منجز جزئيًا**
```text
# cli/src/main.rs:82-88
surface "name": {npts} pts, {ntri} triangles, {n_edges} TIN edges, plan area {area} -> out.dxf
```
نفس النطاق في الإضافة: `src/dispatch.rs:546-549`. **غائب:** min/max elevation، مساحة 3D، انحدار.

### 8.2 أوامر CLI المتاحة — **منجز (7 أوامر)**
`surface, volume, datum, rts, helmert, resect, inverse` — `cli/src/main.rs:47-57`.
- `surface` يكتب DXF (TIN + نقاط) — 62-89.
- `volume` حجم دقيق (+ `--grid` اختياري) مع إخراج DXF — 94-154.
- `datum` قطع/تعبئة ضد مستوى + مقطع + DXF — 156-203.
- **لا يوجد** أمر تصدير/كنتور/breakline في CLI.

---

## 9) LandXML / DEM

### 9.1 استيراد LandXML TIN — **منجز**
- المحرك: `read_surfaces` — `crates/landsurvey/src/landxml.rs:28`، `parse_surface` — 51، واستخراج اسم السطح — `NamedSurface` 22 و126.
- الوصل: أمر مزدوج `LS_LANDXML | LANDXMLIMPORT` — `src/dispatch.rs:90` (arm) و`import_landxml` — 552-602: كيان `Mesh`، قاعدة `<P>` هي Northing Easting [Elev] → X=E/Y=N/Z=Z، تخطي وجوه `<F i="1">` غير المرئية — 554-556.

### 9.2 استيراد DEM — **غير منجز** (انظر 5.4)

### 9.3 الاسم / الوحدات / نظام الإحداثيات
- **الاسم: محفوظ** — `NamedSurface.name` (`landxml.rs:22,126`) يصبح اسم الطبقة `LS-TIN-<NAME>` واسم السطح المخزّن بالاسم — `src/dispatch.rs:556-557` + `put_surface` `src/state.rs:49`.
- **الوحدات: غير محوَّلة وغير مخزَّنة** — «units are imported as-is» — تعليق مرجعي `src/dispatch.rs:554-555`؛ بحث `Units` في `landxml.rs`: صفر (لا حقل وحدات في `NamedSurface`).
- **نظام الإحداثيات: غير منجز** — لا metadata CRS في أي مكان.

---

## 10) الأداء

### 10.1 التعقيد — كما هو في الكود
- توليد Delaunay: مسح كامل للمثلثات لكل نقطة + `find` خطّي — `surface.rs:616-642` و627 ⇒ O(n²) عمليًا.
- التراكب بين سطحين: `O(Nt*Nb)` موثّق — `surface.rs:349`.
- الاستعلام الموضعي: `interpolate_z` O(T) — 163؛ `find_containing_triangle` O(T) — 182.
- لا يوجد spatial index (KD-tree / grid hashing) في أي ملف.

### 10.2 اختبارات ضغط/أحجام كبيرة — **غير منجز**
- لا يوجد `criterion` ولا `benches/` (التخطيط فقط: `FEATURELINE_PARITY_PLAN/README.md:126` و`PHASE5_..._TechPlan.md:555`).
- أكبر حجم مُختبَر فعليًا: fixture ذهبي `fixtures/road_surface.landxml` — **1770 نقطة / 3234 مثلث**.

### 10.3 تحسينات أداء منفَّذة — **محدودة**
- الاحتفاظ بالسطح في الذاكرة لتفادي إعادة القراءة: «retained so commands like LS_VOLUME can operate on it by name» — `src/state.rs:27-28`.
- إعادة استخدام الطوبولوجياالمتطابقة كمسار سريع `volume_between_shared` — `surface.rs:273-289`.
- لا توجد تحسينات أخرى (لا caching للحواف، لا parallelism، لا `unsafe` إطلاقًا في المستودع — بحث مؤكد).

---

## 11) الاختبارات (نتائج فعلي)

### 11.1 تشغيل `cargo test -p landsurvey` — **مُنفَّذ وناجح**
```text
test result: ok. 43 passed; 0 failed   (وحدات landsurvey)
test result: ok. 5 passed; 0 failed    (golden: road_surface_volume_golden)
test result: ok. 1 passed; 0 failed    (integration: volume_pnezd)
= 49 ناجحًا، 0 فاشل  (+ 0 doc-tests)
```
9 تحذيرات (كلها `unused`): `HashMap` (`entity.rs:7`)؛ `Node, Tri` (`breakline.rs:6`)؛ `FeatureLineSyncReport, FeatureVertex, Point3d, ZSource` (`breakline.rs:7`)؛ معامل `result` غير مستخدم في `insert_breakline_vertices` (`breakline.rs:61`)؛ ومتغيّران `tri1, tri2` مُسجَّلان لا يُستخدمان (`breakline.rs:238-239` — يُسجَّلان ثم يُعاد استخدام `surface.triangles[...]` مباشرة، أثر قراشة خفيف).

### 11.2 الحالات الحدية المغطاة — **منجز جزئيًا**
موجودة في وحدة `surface.rs` (الوحدة 661-834): مربع → مثلثان، إزالة تكرار الحافة المشتركة، قطع/تعبئة عند datum، مقطع datum، تطابق الطريقة الشبكية مع الدقيقة، composite بطوبولوجيا مشتركة ومختلفة، حدّ فارغ موجود/غير موجود.
**غائبة:** نقاط مكررة، `n < 3`، نقاط مستقيمة، breakline متقاطعة/معزولة، نقاط خارج حدود، إزالة breakline.

### 11.3 توزيع التغطية
- وحدات: `surface` 10، `transform` 9، `pnezd` 8، `resection` 6، `cogo` 3، `viz` 4، `dxf` 2، `landxml` 1 = **43**.
- تكامل: golden 5 + `volume_pnezd` 1 = **6**.
- **`featureline/*.rs` (breakline/entity/mod): صفر اختبارات.**

### 11.4 أخطاء/خلل لوحظ أثناء المراجعة
- **خلل تسمية في زر الشريط:** الزر يرسل `LS_FEATURELINE_CREATE` — `src/ribbon.rs:150,153`، بينما المسجَّل في الموزّع `LS_FEATURELINE` فقط — `src/dispatch.rs:121`، ولا يوجد arm لـ `LS_FEATURELINE_CREATE` في `dispatch_verb` (57-126)، رغم أن رسالة `LS_HELLO` (64) وتعليقة الدالة (1444) تعلنان الاسم الأول. النتيجة المتوقعة: الزر يرسل أمرًا لا يقابله سجل ⇒ «unknown command».
- تحذيرات الـ 9 unused أعلاه (قراشة).

### 11.5 بيئة الاختبار والاعتمادات
- الاعتمادات الخارجية للمحرك: `serde`, `serde_json`, `chrono` فقط — `crates/landsurvey/Cargo.toml:11-14`.
- صفر `unsafe` في كل ملفات `.rs` (بحث مؤكد).
- الاختبارات: `#[cfg(test)]` داخل الوحدات + `tests/` تكاملية (golden + volume_pnezd)؛ لا CI ولا property-testing (proptest) مرفقَين.

---

## 12) الأوامر والشريط والأوامر التفاعلية

### 12.1 الأوامر المتاحة — **14 أمرًا** (`src/dispatch.rs:57-126`)
`LS_HELLO` (59)، `LS_PNEZD` (70)، `LS_AUTOLABEL` (74)، `LS_IMPORTPLAN` (78)، `LS_INVERSE` (82)، `LS_VOLUME` (86)، `LS_SURFACE` (90)، `LS_LANDXML|LANDXMLIMPORT` (بين 90 و101)، `LS_DATUM` (101)، `LS_RTS` (105)، `LS_HELMERT` (109)، `LS_RESECT` (113)، `LS_LIST` (117)، `LS_FEATURELINE` (121).
لا أوامر: تعديل سطح، كنتور، انحدار، إحصاءات متقدمة، حدود، لصق، تصدير.

### 12.2 شريط الأوامر (Ribbon) — **منجز (6 مجموعات)** — `src/ribbon.rs:21-157`
- Points (27-44): Import PNEZD، List Points.
- Plan (46-59): Import Plan.
- COGO (60-68): Inverse.
- Transform (69-94): RTS، Helmert.
- **Surface (95-145): Build Surface، LandXML، Volume، To Datum.**
- **Feature Line (146-156): زر واحد** — مع ملاحظة خلل التسمية في 11.4.
- لا أزرار: كنتور، انحدار، إحصاءات، حدود، تصدير.

### 12.3 الأوامر التفاعلية — **منجز واحد فقط**
- `FeatureLineCreateCommand` — `src/interactive.rs:27-49`، حالات `CreateState { PickingFirstPoint, PickingNextPoint, ConfirmingZ { tin_z }, Finished }` — يقرأ Z من التثليث ويسمح بقيمة مخصصة.
- **قيدان:** (1) يُنهي بـ `CommitAndEnd(LwPolyline)` فقط — `interactive.rs:191-193` دون XDATA أو breakline؛ (2) يستخدم أول سطح متاح دائمًا مع تعليق «For now, use the first surface» — `src/dispatch.rs:1468-1469`.
- هو الأمر التفاعلي الوحيد في المستودع (بحث: تطبيق وحيد لـ `InteractiveCommand` — `interactive.rs:122`).

---

## 13) الفجوات العشر — التقييم والجدوى والأولويات

| # | الميزة | الحالة | الدليل/السبب |
|---|---|---|---|
| 1 | Breaklines | **جزئي** (محرك منجز، غير موصول + خلخ إدراج) | §3: لا مستهلك في `src/`، و`breakline.rs:96-186` لا تُدرج عقدًا جديدة |
| 2 | حدود السطح (boundary) | **غير منجز** | لا حقل حدود في `Surface` (64-67)؛ لا دالة قص |
| 3 | لصق/دمج أسطح | **غير منجز** | §5.2 |
| 4 | تعديل السطح | **جزئي/تقريبي** | §4: بلا APIs؛ `remove_breakline` تقريبي معترف به (`surface.rs:206`) |
| 5 | كنتورات تلقائية | **غير منجز** | §6: المقطع الوحيد مرتبط بالحجم (`surface.rs:141-158`) |
| 6 | تحليل انحدار/ارتفاع | **غير منجز** | §6: grep صفر |
| 7 | Volume surface حقيقي بحدود | **غير منجز** | §7.1 |
| 8 | Composite volumes | **منجز** | `surface.rs:344-353` + شبكة 291-332 + shared 275-289 |
| 9 | إحصاءات السطح | **جزئي** | pts/tris/edges/plan فقط — `dispatch.rs:546-549` |
| 10 | تصدير LandXML/DEM | **غير منجز** | بحث export: صفر نتائج (استيراد فقط — §9) |

### 13.2 أسوأ فجوة تقنية
**فجوة الـ breakline** بأبعادها معًا: (أ) خوارزميًا، `retriangulate_with_constraints` لا تضمن إنشاء حواف مقيدة لعقد جديدة (`breakline.rs:96-186`)؛ (ب) تشغيليًا، لا شيء في الواجهة يستدعي `add_breakline` إطلاقًا (`src/` خالٍ من الاستدعاء)؛ (ج) صفر اختبارات تغطيها. أي أن أثقل قدرة مطبوعة (constrained Delaunay matching Civil 3D) لا تُختبر ولا تُستدعى وقد لا تُصحّح التحويج. تبعاتها تصل للحجوم: أي سطح به breakline غير موصول يعطي cut/fill غير صحيح.

### 13.3 هل البنية قابلة للتوسع؟ — **نعم، بوضوح**
- محرك مستقل host-free (`crates/landsurvey`) بلا `unsafe` وبدخل `serde/chrono` فقط (`Cargo.toml:11-14`) ⇒ إضافة قدرات لا تمس المhost.
- النموذج بحقول `pub` (`surface.rs:65-66`) ⇒ إضافة حدود/سجل دون كسر التوافق.
- نقطة توسع واحدة للأوامر: `dispatch_verb` match واحد (`dispatch.rs:57-126`) + زر `ribbon.rs` + حالة `state.rs` ⇒ نمط «verb + دالة + زر» مكرر 14 مرة، سهل التكرار.
- حزم featureline منفصلة (`featureline/mod.rs:14`) جاهزة للاستهلاك بمجرد الوصل.

### 13.4 الخطوات التالية المقترحة (أولوية)
1. **إصلاح breakline من البداية:** ربط `add_breakline` في مسارَي `featureline_create` التفاعلي والـ CSV (مع XDATA)، + إصلاح الإدراج الخوارزمي (إما Bowyer–Watson محلي أو إعادة تثليث المنطقة المتأثرة `find_affected_triangles` `surface.rs:229-251`)، + أول مجموعة اختبارات `featureline` (كسر/تقاطع/معزول/نوعان من الأنواع الستة).
2. **إصلاح خلل التسمية `LS_FEATURELINE_CREATE`/`LS_FEATURELINE`** (ribbon.rs:150 ↔ dispatch.rs:121) واستدعاء `build_xdata` في المسار التفاعلي — إصلاحان صغيران بأثر مباشر على الموثوقية.
3. **حدود السطح + حجم مقيّد بالحدود** (يشفع فجوتين 2 و7 معًا): حقل boundary في `Surface` + قص المثلثات عند الحد في `volume_to_datum`/`cut_fill` (`surface.rs:122-158`) + أمران `LS_BOUNDARY`/`LS_VOLUME ... in <boundary>`.
4. **إحصاءات كاملة + كنتور عام:** min/max/متوسط Z ومساحة 3D وانحدار (تُضاف فورًا إلى مخرجات `build_surface` `dispatch.rs:546-549`)، ثم مستخرج كنتور `LS_CONTOUR <elev>` يعيد المقاطع بنفس منطق `cut_fill_to_datum_detailed` (`surface.rs:141-158`).
5. **تصدير LandXML** (معكوسة لـ `landxml.rs:28-126`) + حدود اختبار حالات ح §11.2 + benchmark بـ `criterion` (كما في التخطيط `FEATURELINE_PARITY_PLAN/README.md:126`) لضبط عتبات الأداء قبل رفع أحجام الاختبار فوق 1770 نقطة.

---

*أُعدَّت الوثيقة بقراءة مصدرية مباشرة وتشغيل فعلي لحزمة الاختبارات؛ كل ادعاء إيجابي مسنود بسطر فعلي، وكل ادعاء سلبي («غير منجز») ناتج عن بحث نصي مؤكد في المستودع.*
