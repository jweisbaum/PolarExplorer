// @vitest-environment happy-dom
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { TEST_BLEND } from "../testBlend";
import type { ProjectSummary } from "../generated/ProjectSummary";
const invoke=vi.hoisted(()=>vi.fn());
const reportFailure=vi.hoisted(()=>vi.fn());
vi.mock("@tauri-apps/api/core",()=>({invoke}));
vi.mock("../errors",()=>({reportFailure}));
vi.mock("./BoatWorkspace",()=>({default:({project,split,visible}: {project:ProjectSummary;split:boolean;visible:boolean})=><div data-testid="workspace" data-mode={split?"3d":"all"} data-visible={visible}>{project.boat_name}</div>}));
const {default:FleetWorkspace}=await import("./FleetWorkspace");
(globalThis as {IS_REACT_ACT_ENVIRONMENT?:boolean}).IS_REACT_ACT_ENVIRONMENT=true;
let host:HTMLDivElement;let toolbar:HTMLDivElement;let status:HTMLDivElement;let root:Root;let docs:ProjectSummary[];let active:ReturnType<typeof vi.fn>;
let deleted: { doc: ProjectSummary; index: number }[];
function doc(id:number,name:string):ProjectSummary{return {id,name:"Fleet",boat_name:name,boat_notes:"",revision:1,path:null,dirty:true,sources:[],can_undo:false,can_redo:false,undo_label:null,redo_label:null,use_corrected:true,stokes_drift:false,blend:TEST_BLEND};}
/** The shell handing down a newer root summary, as it does on `document://changed`. */
let setRoot:(project:ProjectSummary)=>void;
function FleetHarness() {
 const [project,setProject]=useState(docs[0]!);
 setRoot=setProject;
 return <FleetWorkspace project={project} settings={null} onSettings={()=>{}} onProject={setProject} changedBoat={null} onActive={active} toolbarHost={toolbar} statusHost={status} onReplace={(_,next)=>setProject(next)}/>;
}
async function click(selector:string){await act(async()=>{(host.querySelector<HTMLButtonElement>(selector) ?? toolbar.querySelector<HTMLButtonElement>(selector) ?? status.querySelector<HTMLButtonElement>(selector))!.click();});}
beforeEach(async()=>{
 docs=[doc(101,"Alpha")];deleted=[];active=vi.fn();invoke.mockReset();reportFailure.mockReset();
 invoke.mockImplementation(async(command:string,args?:Record<string,unknown>)=>{
  if(command==="boat_tabs")return {project_id:docs[0]!.id,name:"Fleet",dirty:true,can_restore:deleted.length>0,tabs:docs.map(d=>({id:d.id,name:d.boat_name,details:{}}))};
  if(command==="project_summary"){
   const found=docs.find(d=>d.id===(args?.boatContext??docs[0]!.id));
   // As the application answers for a boat the project does not hold.
   if(!found)throw {kind:"no-project",message:"No project is open."};
   return found;
  }
  if(command==="add_boat"){const next=doc(101+docs.length,args!.name as string);docs.push(next);return next;}
  if(command==="rename_boat"){const d=docs.find(d=>d.id===args!.boatContext)!; const next={...d,boat_name:args!.name as string,revision:d.revision+1};docs=docs.map(d=>d.id===next.id?next:d);return next;}
  if(command==="delete_boat"){const index=docs.findIndex(d=>d.id===args!.boatId);deleted.push({doc:docs[index]!,index}); docs=docs.filter(d=>d.id!==args!.boatId);return docs[0];}
  if(command==="restore_boat"){const removed=deleted.pop()!;docs.splice(removed.index,0,removed.doc);return docs[0];}
  throw new Error(command);
 });
 host=document.createElement("div");toolbar=document.createElement("div");status=document.createElement("div");document.body.append(toolbar,host,status);root=createRoot(host);
 await act(async()=>root.render(<FleetHarness/>));
});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();toolbar.remove();status.remove();});
it("adds isolated tabs and sends rename and summary requests to the selected boat",async()=>{
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 await click('[data-feature="boats:add"]');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(2);expect(active).toHaveBeenLastCalledWith(102);
 expect(toolbar.querySelector('[data-feature="boats:add"]')).not.toBeNull();
 expect(host.querySelector('[data-feature="boats:rename"]')).toBeNull();
 await act(async()=>host.querySelector('[role="tab"][aria-selected="true"]')!.dispatchEvent(new MouseEvent("dblclick",{bubbles:true})));
 const input=host.querySelector<HTMLInputElement>('[data-feature="boats:name"]')!;
 await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value")!.set!.call(input,"Bravo");input.dispatchEvent(new Event("input",{bubbles:true}));});
 await act(async()=>input.dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",bubbles:true})));
 expect(invoke).toHaveBeenCalledWith("rename_boat",{boatContext:102,name:"Bravo"});
 expect(docs[0]!.boat_name).toBe("Alpha");expect(docs[1]!.boat_name).toBe("Bravo");
 expect(status.querySelector('[data-feature="boats:export-all"]')).not.toBeNull();
});
/** The × of a tab, by the tab's name. */
const closeOf=(name:string)=>[...host.querySelectorAll<HTMLElement>('.boat-tab')].find(tab=>tab.querySelector('[role="tab"]')!.textContent===name)!
 .querySelector<HTMLButtonElement>('[data-feature="boats:delete"]')!;
it("closes a tab from the × on it, after asking", async()=>{
 // The only tab cannot be closed: its × is there, and says why not.
 expect(host.querySelector('.boat-actions')).toBeNull();
 expect(closeOf("Alpha").disabled).toBe(true);
 expect(closeOf("Alpha").title).toBe("A project must contain at least one polar");
 await click('[data-feature="boats:add"]'); await click('[data-feature="boats:add"]');
 expect([...host.querySelectorAll('.boat-tab')].map(tab=>tab.querySelectorAll('[data-feature="boats:delete"]').length)).toEqual([1,1,1]);
 const close=closeOf("Polar 2");
 expect(close.disabled).toBe(false);
 expect(close.textContent).toBe("×");
 expect(close.getAttribute("aria-label")).toBe("Delete Polar 2");
 expect(close.title).toBe("Delete Polar 2");
 // Polar 3 is on show; closing Polar 2 asks first and changes nothing until answered.
 await act(async()=>close.click());
 expect(invoke.mock.calls.filter(c=>c[0]==="delete_boat")).toHaveLength(0);
 const dialog=host.querySelector('[role="dialog"]')!;
 expect(dialog.textContent).toContain("Delete Polar 2");
 expect(dialog.textContent).toContain("Remove this polar and its sources from the project?");
 // Cancel keeps it.
 await act(async()=>[...dialog.querySelectorAll("button")].find(b=>!b.classList.contains("danger"))!.click());
 expect(host.querySelector('[role="dialog"]')).toBeNull();
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(3);
 expect(invoke.mock.calls.filter(c=>c[0]==="delete_boat")).toHaveLength(0);
 // Asked again and confirmed, that tab goes, and the one on show stays on show.
 await act(async()=>closeOf("Polar 2").click());
 await click('[role="dialog"] button.danger');
 expect(invoke).toHaveBeenCalledWith("delete_boat",{projectId:101,boatId:102});
 expect([...host.querySelectorAll('[role="tab"]')].map(tab=>tab.textContent)).toEqual(["Alpha","Polar 3"]);
 expect(host.querySelector('[role="tab"][aria-selected="true"]')!.textContent).toBe("Polar 3");
 // Undo is offered in the tab row.
 expect(host.querySelector('.boat-tabs-row [data-feature="boats:restore"]')!.textContent).toBe("Undo Delete Polar");
 // Clicking the × does not select or rename the tab it is on.
 await act(async()=>closeOf("Alpha").click());
 expect(host.querySelector('[role="tab"][aria-selected="true"]')!.textContent).toBe("Polar 3");
 expect(host.querySelector('[data-feature="boats:name"]')).toBeNull();
});
it("adds a tab containing a named polar and offers undoable deletion", async()=>{
 const add=toolbar.querySelector<HTMLButtonElement>('[data-feature="boats:add"]')!;
 expect(add.textContent).toBe("Add Tab");
 await click('[data-feature="boats:add"]');
 expect(invoke).toHaveBeenCalledWith("add_boat",{name:"Polar 2"});
 expect([...host.querySelectorAll('[role="tab"]')].map(tab=>tab.textContent)).toEqual(["Alpha","Polar 2"]);
 await act(async()=>closeOf("Polar 2").click());
 expect(host.querySelector('[role="dialog"] button.danger')!.textContent).toBe("Delete Polar");
 await click('[role="dialog"] button.danger');
 expect(host.querySelector('[data-feature="boats:restore"]')!.textContent).toBe("Undo Delete Polar");
});
it("puts the layout switch in the top bar as icons, beside the tab controls", async()=>{
 const names:Record<string,string>={"boats:single":"Single view","boats:split":"Split view","boats:four":"Four-way view"};
 for(const [feature,name] of Object.entries(names)){
  expect(host.querySelector(`[data-feature="${feature}"]`)).toBeNull();
  const button=toolbar.querySelector<HTMLButtonElement>(`[data-feature="${feature}"]`)!;
  // An icon and no words: the name is what a pointer or a screen reader is told.
  expect(button.querySelector("svg")).not.toBeNull();
  expect(button.textContent).toBe("");
  expect(button.getAttribute("aria-label")).toBe(name);
  expect(button.title).toBe(name);
 }
 expect(toolbar.querySelector('[data-feature="boats:single"]')!.getAttribute("aria-pressed")).toBe("true");
 // In the same row as the tab controls, after them.
 const order=[...toolbar.querySelectorAll("[data-feature]")].map(el=>el.getAttribute("data-feature"));
 expect(order).toEqual(["boats:add","boats:import-tab","boats:export-tab","boats:single","boats:split","boats:four"]);
 await click('[data-feature="boats:add"]');await click('[data-feature="boats:split"]');
 expect(toolbar.querySelector('[data-feature="boats:split"]')!.getAttribute("aria-pressed")).toBe("true");
 expect(toolbar.querySelector('[data-feature="boats:single"]')!.getAttribute("aria-pressed")).toBe("false");
});
it("offers Export all beside the version, once there is more than one polar", async()=>{
 expect(status.querySelector('[data-feature="boats:export-all"]')).toBeNull();
 await click('[data-feature="boats:add"]');
 expect(host.querySelector('[data-feature="boats:export-all"]')).toBeNull();
 const all=status.querySelector<HTMLButtonElement>('[data-feature="boats:export-all"]')!;
 expect(all.textContent).toBe("Export all tabs");
 await click('[data-feature="boats:export-all"]');
 expect(host.querySelector('.boat-export-dialog')).not.toBeNull();
});
it("populates two and four unique panes, forces 3D, and restores a single tab",async()=>{
 for(let i=0;i<3;i++)await click('[data-feature="boats:add"]');
 await click('[data-feature="boats:split"]');
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
 expect(host.querySelectorAll('.boat-pane:not([hidden]) [data-mode="3d"]')).toHaveLength(2);
 await click('[data-feature="boats:four"]');
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
 const panes=[...host.querySelectorAll('.boat-pane:not([hidden])')];expect(panes).toHaveLength(4);
 expect(new Set(panes.map(p=>p.getAttribute("data-boat-id"))).size).toBe(4);
 await click('[data-feature="boats:single"]');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(4);
 expect(host.querySelectorAll('.boat-pane:not([hidden]) [data-mode="all"]')).toHaveLength(1);
});
it("cancels an inline rename and deletes/restores only the chosen boat", async()=>{
 expect(closeOf("Alpha").disabled).toBe(true);
 await click('[data-feature="boats:add"]');
 await act(async()=>host.querySelector('[role="tab"][aria-selected="true"]')!.dispatchEvent(new KeyboardEvent("keydown",{key:"F2",bubbles:true})));
 const input=host.querySelector<HTMLInputElement>('[data-feature="boats:name"]')!;
 await act(async()=>input.dispatchEvent(new KeyboardEvent("keydown",{key:"Escape",bubbles:true})));
 expect(invoke.mock.calls.filter(c=>c[0]==="rename_boat")).toHaveLength(0);
 await act(async()=>closeOf("Polar 2").click());
 await click('[role="dialog"] button.danger');
 expect(invoke).toHaveBeenCalledWith("delete_boat",{projectId:101,boatId:102});
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 expect(host.querySelector('[data-boat-id="102"]')).toBeNull();
 await click('[data-feature="boats:restore"]');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(2);
});
it("compares the remaining polars after the first is closed, and restores it", async()=>{
 await click('[data-feature="boats:add"]'); await click('[data-feature="boats:add"]');
 await act(async()=>closeOf("Alpha").click()); await click('[role="dialog"] button.danger');
 expect(docs[0]!.id).toBe(102);
 await click('[data-feature="boats:split"]');
 expect(host.querySelector('.boat-grid.boats-2')).not.toBeNull();
 // The comparison layouts have no tab row, so nothing to close or restore there.
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
 expect(host.querySelector('[data-feature="boats:delete"]')).toBeNull();
 expect(host.querySelectorAll('.boat-pane:not([hidden])')).toHaveLength(2);
 expect(host.querySelector('[data-boat-id="101"]')).toBeNull();
 await click('[data-feature="boats:single"]');
 await click('[data-feature="boats:restore"]');
 expect(docs[0]!.id).toBe(101);
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(3);
});
/** What the MCP service's `document://changed` does: the shell hands down the root's newer summary. */
async function changedOutside(){docs=docs.map((d,i)=>i===0?{...d,revision:d.revision+1}:d);await act(async()=>setRoot(docs[0]!));await act(async()=>{});}
it("drops the pane of a boat removed from outside, and shows another without a failure", async()=>{
 await click('[data-feature="boats:add"]');
 expect(host.querySelector('.boat-pane.active')!.getAttribute("data-boat-id")).toBe("102");
 // An MCP client's boat_remove: the document changed under the interface.
 docs=docs.filter(d=>d.id!==102);
 await changedOutside();
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 expect(host.querySelector('[data-boat-id="102"]')).toBeNull();
 expect(host.querySelector('.boat-pane.active:not([hidden])')!.getAttribute("data-boat-id")).toBe("101");
 expect(active).toHaveBeenLastCalledWith(101);
 expect(reportFailure).not.toHaveBeenCalled();
});
it("shows a boat restored from outside after the interface deleted it", async()=>{
 await click('[data-feature="boats:add"]');
 await act(async()=>closeOf("Polar 2").click()); await click('[role="dialog"] button.danger');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 // An MCP client's boat_restore.
 const removed=deleted.pop()!;docs.splice(removed.index,0,removed.doc);
 await changedOutside();
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(2);
 await act(async()=>host.querySelectorAll<HTMLButtonElement>('[role="tab"]')[1]!.click());
 await act(async()=>{});
 const pane=host.querySelector('.boat-pane.active:not([hidden])')!;
 expect(pane.getAttribute("data-boat-id")).toBe("102");
 expect(pane.textContent).toContain("Polar 2");
 expect(reportFailure).not.toHaveBeenCalled();
});
