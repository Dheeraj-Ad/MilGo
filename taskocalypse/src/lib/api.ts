export type User={id:string;email:string;name:string;gmail:boolean;terms:boolean;xp:number;completed:number;expiresAt?:number;lastSyncedAt?:number;syncError?:string};
export type Verification={verificationRequired:true;email:string;expiresAt:number;resendAfter:number};
export type Mail={id:string;threadId:string;from:string;subject:string;body:string;date:string;unread:boolean;community:boolean};
export async function api<T>(path:string,options:RequestInit={}):Promise<T>{
  const response=await fetch(`/api${path}`,{...options,credentials:'include',headers:{'Content-Type':'application/json','X-Taskocalypse':'town',...options.headers}});
  if(!response.headers.get('content-type')?.includes('application/json'))throw new Error('The Rust server is not running. Start it with the setup instructions, or explore the demo town.');
  const data=await response.json();if(!response.ok){if(response.status===401&&(path.startsWith('/mail')||path==='/auth/me'))window.dispatchEvent(new Event('tq-session-expired'));throw new Error(data.error||'Something went wrong. Please try again.');}return data;
}
export const sampleMail:Mail[]=[
  {id:'demo-1',threadId:'demo-1',from:'Aarav Shah <aarav@example.com>',subject:'Coffee and a little project catch-up?',body:'Hey there!\n\nWelcome to the neighbourhood. I’ve been putting together a little community project and would love your thoughts. Are you free for a coffee this Friday?\n\nNo rush — let me know what works for you.\n\nCheers,\nAarav',date:'Today',unread:true,community:false},
  {id:'demo-2',threadId:'demo-2',from:'Maya Chen <maya@example.com>',subject:'Your design feedback would mean a lot',body:'Hi neighbour!\n\nI’ve finished the first draft of our student club poster. Could you take a look and send me your thoughts? A fresh pair of eyes always helps.\n\nThanks,\nMaya',date:'Today',unread:true,community:false},
  {id:'demo-3',threadId:'demo-3',from:'Campus Creators <club@example.com>',subject:'The Makers Festival is coming to town!',body:'Calling all curious minds!\n\nJoin the student community for a day of building, sharing, and making new friends. Bring your ideas — we’ll bring the snacks.\n\nSaturday · 4 PM · Student Commons\n\nEveryone is welcome.',date:'Today',unread:true,community:true},
  {id:'demo-4',threadId:'demo-4',from:'Dev Society <devs@example.com>',subject:'Build together: weekend hack night',body:'Hello builders!\n\nOur next community hack night is this weekend. Team up, try something new, and ship a tiny project. Beginners are very welcome.\n\nSunday · 6 PM · Innovation Lab',date:'Yesterday',unread:true,community:true},
  {id:'demo-5',threadId:'demo-5',from:'Sam Patel <sam@example.com>',subject:'A quick question about tomorrow',body:'Hey!\n\nJust checking whether we’re still meeting at 10 tomorrow. I can bring the notes from our last session.\n\nSee you soon,\nSam',date:'Yesterday',unread:true,community:false},
];
