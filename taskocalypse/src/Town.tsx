import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { discoveries, residents, type District } from './townData';

export { discoveries, residents } from './townData';
export type { District } from './townData';
interface Props { onInteract:(id:District)=>void; onDiscover:(id:string)=>void; foundDiscoveries:string[]; active:District|null; night:boolean; zoom:number; reset:number; reducedMotion:boolean; }

export default function Town({onInteract,onDiscover,foundDiscoveries,active,night,zoom,reset,reducedMotion}:Props) {
  const mount=useRef<HTMLDivElement>(null);
  const callback=useRef(onInteract); callback.current=onInteract;
  const discoveryCallback=useRef(onDiscover);discoveryCallback.current=onDiscover;
  const settings=useRef({active,night,zoom,reset,reducedMotion,foundDiscoveries});settings.current={active,night,zoom,reset,reducedMotion,foundDiscoveries};
  const [failed,setFailed]=useState(false);
  useEffect(()=>{
    const host=mount.current!;
    let renderer:THREE.WebGLRenderer;
    try { renderer=new THREE.WebGLRenderer({antialias:true,alpha:true}); } catch {setFailed(true);return;}
    renderer.setPixelRatio(Math.min(window.devicePixelRatio,1.5));
    renderer.shadowMap.enabled=true;renderer.shadowMap.type=THREE.PCFSoftShadowMap;
    renderer.outputColorSpace=THREE.SRGBColorSpace;renderer.toneMapping=THREE.ACESFilmicToneMapping;renderer.toneMappingExposure=1.35;
    renderer.domElement.setAttribute('aria-label','Interactive 3D town. Use arrow keys or WASD to walk, E to talk, and drag to look around.');
    renderer.domElement.tabIndex=0;host.appendChild(renderer.domElement);
    const scene=new THREE.Scene();scene.fog=new THREE.Fog('#e9edda',38,85);
    const camera=new THREE.OrthographicCamera(-15,15,12,-12,.1,200);camera.position.set(23,24,29);
    const controls=new OrbitControls(camera,renderer.domElement);controls.target.set(0,0,0);controls.enableDamping=true;controls.enablePan=false;controls.minPolarAngle=.3;controls.maxPolarAngle=1.25;controls.minZoom=.65;controls.maxZoom=2.8;controls.enableZoom=true;
    const ambient=new THREE.HemisphereLight('#fff9e6','#819067',3);scene.add(ambient);
    const sun=new THREE.DirectionalLight('#fff3d1',4);sun.position.set(-12,24,10);sun.castShadow=true;sun.shadow.mapSize.set(1024,1024);Object.assign(sun.shadow.camera,{left:-22,right:22,top:22,bottom:-22,near:1,far:75});sun.shadow.normalBias=.035;scene.add(sun);
    const materials=new Map<string,THREE.MeshStandardMaterial>();
    function mat(color:string){if(!materials.has(color))materials.set(color,new THREE.MeshStandardMaterial({color,roughness:.85}));return materials.get(color)!;}
    const geometries:THREE.BufferGeometry[]=[];
    function mesh(g:THREE.BufferGeometry,c:string,x=0,y=0,z=0,parent:THREE.Object3D=scene){geometries.push(g);const m=new THREE.Mesh(g,mat(c));m.position.set(x,y,z);m.castShadow=true;m.receiveShadow=true;parent.add(m);return m;}
    const box=(w:number,h:number,d:number,c:string,x:number,y:number,z:number,p:THREE.Object3D=scene)=>mesh(new THREE.BoxGeometry(w,h,d),c,x,y,z,p);
    const ball=(r:number,c:string,x:number,y:number,z:number,p:THREE.Object3D=scene)=>mesh(new THREE.IcosahedronGeometry(r,1),c,x,y,z,p);
    const cyl=(r:number,r2:number,h:number,c:string,x:number,y:number,z:number,p:THREE.Object3D=scene,n=12)=>mesh(new THREE.CylinderGeometry(r,r2,h,n),c,x,y,z,p);
    // A floating hand-built miniature: every building, tree and resident is real 3D geometry.
    box(24,1.2,20,'#b2b393',0,-.75,0);box(24.2,.32,20.2,'#b7cb8a',0,-.1,0);
    box(24.4,.12,20.4,'#e4d8b5',0,-.43,0);box(23.7,.08,3.15,'#e3d7b9',0,.1,2.4);box(3.15,.08,19.7,'#e3d7b9',0,.11,0);
    box(18,.08,2,'#e3d7b9',0,.1,-4.4);cyl(3,3,.12,'#e3d7b9',0,.12,2.4,scene,48);
    for(let i=0;i<32;i++){box(.52,.05,.31,'#cec6aa',-11.4+i*.73,.17,.93);box(.52,.05,.31,'#cec6aa',-11.4+i*.73,.17,3.87);}
    for(let i=0;i<23;i++){box(.3,.05,.5,'#cec6aa',-1.58,.17,-9+i*.82);box(.3,.05,.5,'#cec6aa',1.58,.17,-9+i*.82);}
    function house(x:number,z:number,w:number,d:number,h:number,wall:string,roof:string,kind:string){
      const group=new THREE.Group();group.position.set(x,0,z);scene.add(group);
      box(w+.15,.3,d+.15,'#a9a990',0,.25,0,group);box(w,h,d,wall,0,h/2+.3,0,group);
      // Gabled roof, with a shallow overhang and ridge.
      const shape=new THREE.Shape();shape.moveTo(-w/2-.3,0);shape.lineTo(0,1.8);shape.lineTo(w/2+.3,0);shape.closePath();
      const roofMesh=mesh(new THREE.ExtrudeGeometry(shape,{depth:d+.6,bevelEnabled:false}),roof,0,h+.25,-d/2-.3,group);
      roofMesh.castShadow=true;
      for(let side=-1;side<=1;side+=2){for(let j=0;j<6;j++){const line=box(w/2+.38,.045,.05,roof,side*(w/4+.1),h+1.13,-d/2+j*(d/5),group);line.rotation.z=side*(-Math.atan2(1.8,w/2+.3));}}
      box(.8,1.65,.12,'#665c43',0,1.12,d/2+.07,group);box(.61,1.43,.13,'#9a7951',0,1.08,d/2+.14,group);ball(.055,'#e9ca74',.2,1.05,d/2+.24,group);
      box(1.25,.15,.65,'#d6c8a8',0,.28,d/2+.3,group);
      for(const dx of [-w*.31,w*.31]){box(.86,.94,.12,'#ede5cb',dx,1.75,d/2+.1,group);box(.65,.73,.14,'#8eacaa',dx,1.75,d/2+.16,group);box(.06,.76,.16,'#f5ecd5',dx,1.75,d/2+.24,group);box(.7,.06,.16,'#f5ecd5',dx,1.75,d/2+.24,group);box(1,.18,.35,'#a0865b',dx,1.15,d/2+.22,group);for(let n=0;n<4;n++)ball(.14,n%2?'#eab6a0':'#c6d68d',dx-.32+n*.21,1.36,d/2+.28,group);}
      box(.55,1.7,.65,'#c6b59b',w*.28,h+1.1,-.4,group);box(.7,.16,.8,'#dad1bb',w*.28,h+1.95,-.4,group);
      if(kind==='mail'){box(1.65,.7,.18,'#f8f0d7',0,h-.25,d/2+.12,group);box(1.05,.42,.2,'#6f8955',0,h-.25,d/2+.23,group);const e=box(.54,.28,.05,'#f4eccf',0,h-.25,d/2+.35,group);e.rotation.z=-.1;}
      if(kind==='workshop'){box(w+.2,.13,1.2,'#9c9a7d',0,1.2,d/2+.6,group);for(const dx of [-1.1,1.1])cyl(.07,.07,1.2,'#7d785f',dx,.6,d/2+1,group);}
      return group;
    }
    house(-6.3,-1.1,4.1,3.6,3.1,'#f2dfb1','#af684d','mail');
    house(-6.7,-6.8,3.7,3,2.6,'#ddd9b8','#6b8979','library');
    house(6.5,-6.5,4,3.2,2.9,'#e8d9bb','#8a8d70','workshop');
    house(6.9,-.7,3.5,3.1,2.8,'#edcf95','#c18b52','bank');
    house(-7,6.9,3.4,2.7,2.5,'#dedbb6','#668572','house');
    house(7.1,7,3.5,2.7,2.4,'#f1d8b8','#ae7158','house');
    // Fountain and translucent pool.
    cyl(1.3,1.45,.32,'#abae9b',0,.3,2.4,scene,32);cyl(1.1,1.1,.08,'#89c1b8',0,.5,2.4,scene,32);
    cyl(.32,.55,.9,'#d6d6bb',0,.85,2.4);cyl(.65,.3,.18,'#d9d9bd',0,1.35,2.4);cyl(.56,.56,.04,'#96d5cb',0,1.46,2.4);ball(.22,'#e7e3c7',0,1.7,2.4);
    const drops:THREE.Mesh[]=[];for(let i=0;i<12;i++){const a=i/12*Math.PI*2;drops.push(ball(.045,'#c1eee0',Math.cos(a)*.65,.9,2.4+Math.sin(a)*.65));}
    function tree(x:number,z:number,size=1){const g=new THREE.Group();g.position.set(x,0,z);g.scale.setScalar(size);scene.add(g);cyl(.14,.22,1.65,'#8b7751',0,.85,0,g,7);ball(1.12,'#829d63',0,2.1,0,g);ball(.83,'#9eb578',-.38,2.7,0,g);ball(.75,'#b0c188',.43,2.5,.24,g);}
    [[-10,-7],[-10,-3],[-10,2],[-10,7],[-3,-8],[2.7,-8],[10,-7],[10,-3],[10,3],[10,7],[-3,7],[3,7],[-3,-5.8]].forEach(([x,z],i)=>tree(x,z,.8+(i%3)*.13));
    function pine(x:number,z:number){cyl(.12,.17,1.4,'#877b55',x,.7,z);for(let i=0;i<3;i++)cyl(.02,1-i*.19,1.5,'#6f9276',x,1.4+i*.65,z,scene,7);}
    pine(-10.6,-8.7);pine(10.5,-8.6);pine(-10.4,8.5);pine(10.5,8.6);
    function fence(x:number,z:number,length:number){for(let i=0;i<=length;i++)box(.12,.7,.13,'#f2e7c8',x+i*.55,.5,z);box(length*.55,.12,.1,'#eee1bc',x+length*.275,.56,z);}
    fence(-8.9,4.9,8);fence(4.5,5.1,9);fence(-9,-9.2,8);
    for(let i=0;i<48;i++){const x=Math.sin(i*127.1)*10.5,z=Math.cos(i*71.7)*8.8;if(Math.abs(x)<2||Math.abs(z-2.4)<2||Math.abs(z+4.4)<1.3)continue;for(let j=0;j<3;j++){cyl(.025,.025,.23,'#72915a',x+j*.13,.24,z);ball(.075,['#ecd19a','#e8a9a3','#eee5c6'][i%3],x+j*.13,.37,z);}}
    function bench(x:number,z:number){box(1.4,.13,.46,'#987950',x,.6,z);box(1.4,.4,.1,'#ad8d60',x,.97,z-.2);for(const dx of [-.5,.5])box(.12,.6,.35,'#626c55',x+dx,.3,z);}
    bench(-2.4,5.5);bench(2.8,-2.4);
    // Festival bunting suspended over the main road.
    for(const x of [-4,4]){cyl(.055,.075,4.1,'#8c7956',x,2.05,-4.2);ball(.1,'#e9c975',x,4.15,-4.2);}
    const curve=new THREE.CatmullRomCurve3([new THREE.Vector3(-4,4,-4.2),new THREE.Vector3(0,3.5,-4.2),new THREE.Vector3(4,4,-4.2)]);mesh(new THREE.TubeGeometry(curve,24,.018,4,false),'#8e8263');
    for(let i=0;i<13;i++){const p=curve.getPoint((i+.5)/13);const g=new THREE.BufferGeometry();g.setAttribute('position',new THREE.Float32BufferAttribute([-.2,0,0,.2,0,0,0,-.44,0],3));g.computeVertexNormals();const m=mesh(g,['#c3846b','#e5c771','#8da488','#b6a3bf'][i%4],p.x,p.y,p.z);m.material=new THREE.MeshStandardMaterial({color:['#c3846b','#e5c771','#8da488','#b6a3bf'][i%4],side:THREE.DoubleSide});}
    function character(color:string,x:number,z:number){const g=new THREE.Group();g.position.set(x,.18,z);scene.add(g);cyl(.22,.3,.58,color,0,.68,0,g);ball(.23,'#e8bd90',0,1.22,0,g);ball(.24,'#69513d',0,1.34,-.035,g);box(.33,.18,.2,'#e8bd90',0,1.23,.13,g);for(const side of [-1,1]){cyl(.075,.08,.4,'#5b6258',side*.12,.25,0,g);box(.15,.12,.27,'#675641',side*.12,.07,.065,g);cyl(.07,.085,.42,color,side*.3,.73,0,g);ball(.075,'#e8bd90',side*.32,.49,0,g);ball(.022,'#3f4134',side*.085,1.28,.245,g);}return g;}
    const npcs=residents.map(r=>{const g=character(r.color,r.x,r.z);g.rotation.y=.3;g.userData.district=r.id;g.traverse(o=>o.userData.district=r.id);const ring=mesh(new THREE.TorusGeometry(.48,.035,8,32),r.color,r.x,.21,r.z);ring.rotation.x=Math.PI/2;return {g,ring,r};});
    const found=new Set(foundDiscoveries);
    const keepsakes=discoveries.filter(item=>!found.has(item.id)).map(item=>{const group=new THREE.Group();group.position.set(item.x,0,item.z);scene.add(group);const gem=mesh(new THREE.OctahedronGeometry(.24), '#f1cc6e',0,.72,0,group);gem.castShadow=false;const glint=mesh(new THREE.OctahedronGeometry(.1),'#fff2b0',.08,.76,.08,group);glint.castShadow=false;const ring=mesh(new THREE.TorusGeometry(.38,.025,6,20),'#dfbd68',0,.2,0,group);ring.rotation.x=Math.PI/2;ring.castShadow=false;return {id:item.id,group,gem};});
    const player=character('#467e8e',-1,6);player.scale.setScalar(1.1);
    const marker=mesh(new THREE.ConeGeometry(.15,.32,4),'#f3da86',-1,2.2,6);marker.rotation.z=Math.PI;
    const labels:HTMLButtonElement[]=[];
    for(const npc of npcs){const el=document.createElement('button');el.className='resident-marker';el.innerHTML=`<span class="marker-dot" style="background:${npc.r.color}"></span>${npc.r.name}<span class="marker-action">↗</span>`;el.setAttribute('aria-label',`Talk to ${npc.r.name}, ${npc.r.role}`);el.addEventListener('click',()=>callback.current(npc.r.id));host.appendChild(el);labels.push(el);}
    const cloudGroup=new THREE.Group();scene.add(cloudGroup);for(let k=0;k<4;k++){const cloud=new THREE.Group();cloud.position.set(-15+k*9,8+k%2*2,-12);cloudGroup.add(cloud);for(let j=0;j<4;j++){const m=ball(.8+j%2*.3,'#f5f4e7',j*.9,Math.sin(j)*.3,0,cloud);m.scale.set(1.4,.65,1);m.castShadow=false;}}
    const keys=new Set<string>();
    const down=(e:KeyboardEvent)=>{if((e.target as HTMLElement).closest('input,textarea,select,[role="dialog"]'))return; if(['ArrowUp','ArrowDown','ArrowLeft','ArrowRight',' '].includes(e.key))e.preventDefault();keys.add(e.key.toLowerCase());if(e.key.toLowerCase()==='e'){const closest=[...npcs].sort((a,b)=>a.g.position.distanceTo(player.position)-b.g.position.distanceTo(player.position))[0];if(closest.g.position.distanceTo(player.position)<3)callback.current(closest.r.id);}};
    const up=(e:KeyboardEvent)=>keys.delete(e.key.toLowerCase());const blur=()=>keys.clear();window.addEventListener('keydown',down);window.addEventListener('keyup',up);window.addEventListener('blur',blur);
    const ray=new THREE.Raycaster();let startX=0,startY=0;const pointerDown=(e:PointerEvent)=>{startX=e.clientX;startY=e.clientY;};const click=(e:MouseEvent)=>{if(Math.hypot(e.clientX-startX,e.clientY-startY)>5)return;const rect=renderer.domElement.getBoundingClientRect();ray.setFromCamera(new THREE.Vector2((e.clientX-rect.left)/rect.width*2-1,-(e.clientY-rect.top)/rect.height*2+1),camera);const hit=ray.intersectObjects(npcs.map(n=>n.g),true)[0];if(hit)callback.current(hit.object.userData.district);};renderer.domElement.addEventListener('pointerdown',pointerDown);renderer.domElement.addEventListener('click',click);
    let width=1,height=1;const resize=()=>{width=host.clientWidth;height=host.clientHeight;renderer.setSize(width,height);const aspect=width/height;camera.left=-12*aspect;camera.right=12*aspect;camera.top=12;camera.bottom=-12;camera.updateProjectionMatrix();};const observer=new ResizeObserver(resize);observer.observe(host);resize();
    let frame=0,last=performance.now(),lastZoom=1,lastReset=0;
    function animate(now:number){frame=requestAnimationFrame(animate);const dialogOpen=!!document.querySelector('[role="dialog"]');if(document.hidden||now-last<(dialogOpen?180:32))return;const dt=Math.min((now-last)/1000,.05);last=now;const t=now/1000;
      if(settings.current.zoom!==lastZoom){camera.zoom=settings.current.zoom;camera.updateProjectionMatrix();lastZoom=settings.current.zoom;}
      if(settings.current.reset!==lastReset){camera.position.set(23,24,29);controls.target.set(0,0,0);camera.zoom=1;camera.updateProjectionMatrix();lastReset=settings.current.reset;}
      ambient.intensity=settings.current.night?1.15:3;sun.intensity=settings.current.night?.8:4;
      keepsakes.forEach(item=>{if(found.has(item.id))return;item.group.rotation.y=t*.65;item.gem.position.y=.72+(settings.current.reducedMotion?0:Math.sin(t*2.5)*.11);if(item.group.position.distanceTo(player.position)<1.05){found.add(item.id);scene.remove(item.group);discoveryCallback.current(item.id);}});
      let dx=0,dz=0;if(keys.has('w')||keys.has('arrowup'))dz-=1;if(keys.has('s')||keys.has('arrowdown'))dz+=1;if(keys.has('a')||keys.has('arrowleft'))dx-=1;if(keys.has('d')||keys.has('arrowright'))dx+=1;
      if(!document.querySelector('[role="dialog"]')&&(dx||dz)){const l=Math.hypot(dx,dz);const nx=player.position.x+dx/l*dt*4,nz=player.position.z+dz/l*dt*4;const onPath=Math.abs(nx)<1.35||Math.abs(nz-2.4)<1.35||Math.abs(nz+4.4)<.85;if(onPath&&Math.abs(nx)<11&&Math.abs(nz)<9){player.position.x=nx;player.position.z=nz;}player.rotation.y=Math.atan2(dx,dz);player.position.y=.18+(settings.current.reducedMotion?0:Math.abs(Math.sin(t*12))*.08);}
      marker.position.set(player.position.x,2.15+(settings.current.reducedMotion?0:Math.sin(t*3)*.1),player.position.z);
      npcs.forEach((n,i)=>{if(n.r.id==='mail'&&!dialogOpen&&!settings.current.reducedMotion){const distance=n.g.position.distanceTo(player.position);if(distance<6&&distance>1.8){const direction=player.position.clone().sub(n.g.position);direction.y=0;direction.normalize();n.g.position.addScaledVector(direction,dt*.65);n.g.rotation.y=Math.atan2(direction.x,direction.z);}}else if(!settings.current.reducedMotion)n.g.rotation.y=Math.sin(t*.8+i)*.25;n.ring.position.set(n.g.position.x,.21,n.g.position.z);n.ring.scale.setScalar(settings.current.active===n.r.id?1.3:1);const p=new THREE.Vector3(n.g.position.x,2,n.g.position.z).project(camera);labels[i].style.transform=`translate(-50%, -50%) translate(${(p.x*.5+.5)*width}px,${(-p.y*.5+.5)*height}px)`;labels[i].style.visibility=p.z>1?'hidden':'visible';});
      if(!settings.current.reducedMotion){drops.forEach((d,i)=>d.position.y=.6+((t*.6+i*.07)%1)*.65);cloudGroup.position.x=Math.sin(t*.045)*1.2;}
      controls.update();renderer.render(scene,camera);
    }frame=requestAnimationFrame(animate);
    return()=>{cancelAnimationFrame(frame);observer.disconnect();controls.dispose();window.removeEventListener('keydown',down);window.removeEventListener('keyup',up);window.removeEventListener('blur',blur);renderer.domElement.removeEventListener('pointerdown',pointerDown);renderer.domElement.removeEventListener('click',click);labels.forEach(l=>l.remove());geometries.forEach(g=>g.dispose());materials.forEach(m=>m.dispose());scene.traverse(o=>{if(o instanceof THREE.Mesh&&!Array.from(materials.values()).includes(o.material))o.material.dispose();});renderer.dispose();renderer.domElement.remove();};
  },[]);
  return <div className={`town-canvas ${night?'night':''}`} ref={mount}>{failed&&<div className="webgl-fallback"><h2>Your town is taking a little break.</h2><p>3D needs a browser with WebGL enabled. You can still meet every resident and explore the keepsakes below.</p><div className="discovery-fallback">{discoveries.filter(item=>!foundDiscoveries.includes(item.id)).map(item=><button key={item.id} onClick={()=>onDiscover(item.id)}>Find {item.name}</button>)}</div>{residents.map(r=><button key={r.id} onClick={()=>onInteract(r.id)}>{r.name} · {r.role}</button>)}</div>}</div>;
}
