export function parseCSV(input) {
  const rows=[];let row=[],cell='',quoted=false;
  for(let i=0;i<input.length;i++){const c=input[i];if(c==='"'){if(quoted&&input[i+1]==='"'){cell+='"';i++;}else quoted=!quoted;}else if(c===','&&!quoted){row.push(cell);cell='';}else if((c==='\n'||c==='\r')&&!quoted){if(c==='\r'&&input[i+1]==='\n')i++;row.push(cell);if(row.some(v=>v.trim()))rows.push(row);row=[];cell='';}else cell+=c;}
  if(quoted)throw new Error('The CSV contains an unclosed quote.');row.push(cell);if(row.some(v=>v.trim()))rows.push(row);
  if(rows.length<2)throw new Error('Include a header and at least one expense.');
  const headers=rows[0].map(v=>v.trim().toLowerCase());const desc=headers.indexOf('description'),amount=headers.indexOf('amount'),date=headers.indexOf('date');
  if(desc<0||amount<0)throw new Error('CSV needs description and amount columns (date is optional).');
  return rows.slice(1).map((r,i)=>{const raw=(r[amount]??'').trim();const n=Number(raw);if(!raw||!Number.isFinite(n))throw new Error(`Invalid amount on row ${i+2}.`);return {id:`expense-${i}`,description:r[desc]||'Untitled',amount:n,date:date<0?'':r[date],category:'Unsorted'};});
}
export function exportCSV(rows){const escape=v=>'"'+String(v??'').replaceAll('"','""')+'"';return ['date,description,amount,category',...rows.map(r=>[r.date,/^[=+@\-\t\r]/.test(r.description)?"'"+r.description:r.description,r.amount,r.category].map(escape).join(','))].join('\r\n');}
export function cleanWhitespace(source){const lines=source.split(/\r?\n/);const issues=[];const fixed=lines.map((line,i)=>{if(/[\t ]+$/.test(line))issues.push({line:i+1,message:'Trailing whitespace'});return line.replace(/[\t ]+$/,'');}).join('\n');return {issues,fixed};}
export function fileCategory(name){const ext=name.split('.').pop().toLowerCase();if(['jpg','jpeg','png','gif','webp','svg','heic'].includes(ext))return 'Images';if(['pdf','doc','docx','txt','md','pptx','xlsx','csv'].includes(ext))return 'Documents';if(['mp3','wav','mp4','mov','mkv'].includes(ext))return 'Media';if(['js','ts','tsx','jsx','rs','py','html','css','json'].includes(ext))return 'Code';return 'Other';}
