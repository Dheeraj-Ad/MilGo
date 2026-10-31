export type District='mail'|'files'|'expenses'|'code'|'community';

export const residents: {id:District;name:string;role:string;color:string;x:number;z:number}[] = [
  {id:'mail',name:'Milo',role:'Your mail courier',color:'#568b4d',x:-3.5,z:2.6},
  {id:'files',name:'Poppy',role:'The town librarian',color:'#cb895a',x:-6,z:-3.8},
  {id:'expenses',name:'Jasper',role:'Your coin keeper',color:'#d0a342',x:5.2,z:2.8},
  {id:'code',name:'Ada',role:'The workshop tinkerer',color:'#9d87bd',x:5,z:-4},
  {id:'community',name:'Sunny',role:'The festival host',color:'#db827a',x:0,z:-5.3},
];

export const discoveries = [
  {id:'south-path',name:'Sunlit acorn',x:0,z:4.7},
  {id:'west-walk',name:'Clover charm',x:-8.2,z:2.4},
  {id:'east-walk',name:'Brass button',x:8.2,z:2.4},
  {id:'fountain',name:'Fountain pearl',x:0,z:-.2},
  {id:'north-grove',name:'Moonstone',x:0,z:-7.8},
  {id:'festival-lane',name:'Festival ribbon',x:3.2,z:-4.4},
] as const;
