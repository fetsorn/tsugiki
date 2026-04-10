python3 scripts/scaffold_intent.py ~/translation1/ 

python3 scripts/docx_to_source_md.py \
~/source.docx \ 
~/translation1/ \
--title-pattern "HEADING" 

python3 scripts/decompose.py ~/translation1/ \ 
--headings-json '["Аннотация", "Тематика", "Основные положения...", ...]' \ 
--conclusion Выводы
