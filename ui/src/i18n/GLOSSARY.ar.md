# مسرد الترجمة العربية (Arabic translation glossary)

The words the Arabic interface and help reference use for PolarExplorer's
concepts, fixed so every area says the same thing. Modern Standard Arabic, in
the register of Apple's Arabic software. **Review status of every entry:
machine-drafted, needs a native-speaking sailor.** Entries marked **⚑** are
the ones the translator was least sure of.

**Register.** Buttons and menu items use the verbal noun (مصدر): "حفظ",
"فتح…", "تصدير…", "إلغاء". Tooltips that describe an action also use the
verbal noun ("إزالة هذا المصدر من المشروع"); instructions addressed to the
reader use the imperative ("انقر", "اسحب", "اكتب").

**Punctuation.** Arabic comma ، semicolon ؛ question mark ؟; quotation marks
« ». Keep `…` where the English has it and every `{placeholder}` exactly.
Latin tokens (ORC, TWA, file names, URLs) stay Latin inside the Arabic
sentence; the bidi algorithm places them. Western digits throughout. One
left-to-right mark (U+200E) precedes "-5" in the Scale tooltip so the minus
sign stays in front of the number.

**Never translated:** PolarExplorer, ORC, ORR, TWA, TWS, BSP, VMG, COG, SOG,
TWD, AWA, Expedition, Adrena, YellowBrick, Geovoile, Blue Water Tracks,
GeoJSON, CSV, GRIB, ERA5, MMSI, MCP, WebGL, file extensions (.wpsproj), unit
symbols (kn, m/s, km/h, m, ft, nm, km, MB, GB, s, °), key names (Shift, Esc,
Escape, Enter, Ctrl, F1, F2, Home, End; ⌘ on a Mac).

| English | العربية | Note |
|---|---|---|
| polar | المنحنى القطبي (ج. المنحنيات القطبية) | The data itself. |
| polar plot / polar diagram | المخطط القطبي | The 2D drawing. |
| polar node | عقدة المنحنى القطبي (ج. عُقد) | |
| polar segment | مقطع منحنى قطبي | A track's binned polar. |
| polar tower (3D layout) | البرج القطبي | |
| blend (noun / verb) | الدمج / دمج | ⚑ "المنحنى المدمج" for the blended polar where needed. |
| source | مصدر | |
| weight | الوزن | "ترجيح" for the verb "to weight". |
| overlay | طبقة تعديل | ⚑ |
| output grid | شبكة الناتج | |
| cell | خلية | |
| slice (2D at one TWS) | مقطع | |
| track | مسار | |
| tracker | جهاز التتبع | Also the service. ⚑ |
| sample (track position) | نقطة | "نقطة عيّنة" in technical text; "موقع" for a raw position. |
| dot (drawn sample) | نقطة | "النقاط المرسومة" where the two must be told apart. |
| exclude / include | استبعاد / تضمين | |
| filter (noun / verb) | مرشّح / تصفية، ترشيح | "مرشَّحة" = filtered out. |
| leave out | استبعاد | |
| derive / derived | استنتاج / مستنتَج | |
| given / provided | مُعطى / مزوَّد | |
| true wind | الريح الحقيقية | |
| TWA — true wind angle | زاوية الريح الحقيقية | |
| TWS — true wind speed | سرعة الريح الحقيقية | |
| apparent wind | الريح الظاهرية | |
| BSP — boat speed | سرعة القارب | |
| heading | الاتجاه / اتجاه القارب | ⚑ "السمت" kept for great-circle bearing. |
| course over ground | المسار بالنسبة إلى القاع | |
| through the water | بالنسبة إلى الماء | |
| upwind / downwind | عكس الريح / مع الريح | |
| to tack / a tack | تغيير الاتجاه | ⚑ Literally "change of heading"; a reviewer may prefer "التحويل عكس الريح". |
| to gybe / a gybe | الانعطاف مع الريح | ⚑ |
| beat / run angle | زاوية الإبحار عكس الريح / مع الريح المثلى | ORC. |
| port / starboard | الميسرة / الميمنة | |
| bow / stern | المقدمة / المؤخرة | |
| head / bow / beam / quarter / following seas | أمواج أمامية / من جهة المقدمة / جانبية / من جهة المؤخرة / خلفية | ⚑ |
| wave height / period / direction | ارتفاع / دورة / اتجاه الأمواج | |
| current | التيار | Toward: "{speed} نحو {direction}". |
| tide | المد والجزر | |
| Stokes drift | انجراف ستوكس | |
| leeway | الانحراف الجانبي | ⚑ |
| swell | الموج المتضخّم | ⚑ |
| sea state | حالة البحر | |
| motoring | الإبحار بالمحرك | |
| reanalysis | إعادة التحليل | |
| weather | الطقس | "جلب الطقس…" = Fetch weather…. |
| environment (wind, waves, current) | البيانات البيئية | |
| fetch | جلب | Download in general: تنزيل. |
| ORC certificate | شهادة ORC | |
| catalogue | كتالوج | |
| sail number | رقم الشراع | |
| builder / designer | حوض البناء / المصمّم | |
| beam / draft / displacement (hull) | العرض / الغاطس / الإزاحة | |
| mainsail / genoa / spinnaker | الشراع الرئيسي / شراع الجنوة / شراع السبينكر | |
| sister ship | قارب شقيق | ⚑ |
| class / division | الفئة / القسم | |
| handicap class | فئة الإعاقة | ⚑ |
| race / leg / fleet / event | سباق / مرحلة / أسطول / حدث | |
| start / finish | الانطلاق / خط النهاية | |
| Racing / Finished / Retired / Did not start / Did not finish | في السباق / أنهى السباق / انسحب / لم ينطلق / لم يُنهِ السباق | ⚑ Check against the racing-rules wording. |
| scrape (catalogue) | جمع | ⚑ |
| statistic / percentile / median / mean | الإحصاء / المئين / الوسيط / المتوسط | "المئين التسعون" = 90th percentile. |
| scale / smooth / reset | تحجيم / تنعيم / إعادة الضبط | |
| heat map | خريطة حرارية | |
| threshold | العتبة | |
| operand (Compare A / B) | الطرف | |
| interpolation / extrapolate | الاستيفاء / استقراء | |
| monotone spline | شريحة رتيبة | ⚑ |
| asymmetric polar | منحنى قطبي غير متناظر | |
| project | مشروع | |
| start screen | شاشة البدء | |
| stage / view | العرض | |
| Map / 3D / 2D / Compare | الخريطة / ثلاثي الأبعاد / ثنائي الأبعاد / المقارنة | The verb (`Compare@@verb`) is "مقارنة". |
| navigation (left panel) | لوحة التنقل | |
| settings | الإعدادات | |
| theme | المظهر | Harbour, Midnight, Ocean, Plum, Ember, Paper: المرفأ، منتصف الليل، المحيط، البرقوق، الجمر، الورق. |
| autosave | الحفظ التلقائي | |
| recovered work | العمل المستعاد | |
| unsaved changes | تغييرات غير محفوظة | |
| Save / Don't save / Cancel | حفظ / عدم الحفظ / إلغاء | |
| Open Recent | فتح الأخير | |
| Undo / Redo | تراجع / إعادة | "قابل للتراجع" = undoable. |
| dialog | نافذة | |
| tick / untick | تحديد / إلغاء التحديد | |
| frame (fit on map) | تأطير | |
| equirectangular / orthographic | متساوي المستطيلات / عمودي | Projections. |
| token (MCP) | رمز مميّز | |
| knots / nautical miles | عقدة / ميل بحري | Symbols stay "kn", "nm". |
| Hindsight Sync | Hindsight Sync | اسم منتج، لا يُترجم. |
| reanalysis archive | أرشيف إعادة التحليل | |
| Scan (button) | بحث | |
