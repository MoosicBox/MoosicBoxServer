import{d as yt,c as g,j as ht,o as gt,g as c,a as m,i as n,b as i,S as v,F as w,r as it,t as d,h as qt,n as bt,l as mt,e as $t,x as Lt,f as pt,s as Nt,k as Dt,u as ft}from"./web.o2PnP_jd.js";import{a as It,c as x}from"./Album.B7ZnFso0.js";import{i as Ft,a as h,Q as wt,o as Rt,f as At}from"./api.DZqBGp_p.js";import{d as Bt}from"./index.B1IFpGVk.js";import{P as Mt,Q as Ot}from"./ChangePlaybackTargetModal.BU1Vz2Zf.js";/* empty css                        */var jt=d("<h1 class=artist-page-albums-header>Albums in Library"),C=d("<div class=artist-page-albums>"),Gt=d("<h1 class=artist-page-albums-header>Albums on Tidal"),Ht=d("<h1 class=artist-page-albums-header>EPs and Singles on Tidal"),Vt=d("<h1 class=artist-page-albums-header>Compilations on Tidal"),Jt=d("<h1 class=artist-page-albums-header>Albums on Qobuz"),Kt=d("<h1 class=artist-page-albums-header>EPs and Singles on Qobuz"),Ut=d("<h1 class=artist-page-albums-header>Compilations on Qobuz"),Wt=d("<div class=artist-page-container><div class=artist-page><div class=artist-page-breadcrumbs><a class=back-button href=#>Back</a></div><div class=artist-page-header><div class=artist-page-artist-info><div class=artist-page-artist-info-cover></div><div class=artist-page-artist-info-details><h1 class=artist-page-artist-info-details-artist-title></h1></div></div></div><!$><!/><!$><!/><!$><!/><!$><!/><!$><!/><!$><!/><!$><!/>");function _t(e){const[y,f]=g(),[V,I]=g(),[F,R]=g(),[J,k]=g(),[B,K]=g(),[T,U]=g(),[u,M]=g(),[W,st]=g(),[O,X]=g(),[Y,at]=g();function Z(){return y()??F()??u()}async function S(t){await Promise.all([h.getAllQobuzArtistAlbums(t,st,["LP"]),h.getAllQobuzArtistAlbums(t,X,["EPS_AND_SINGLES"]),h.getAllQobuzArtistAlbums(t,at,["COMPILATIONS"])])}async function P(t){await Promise.all([h.getAllTidalArtistAlbums(t,k,["LP"]),h.getAllTidalArtistAlbums(t,K,["EPS_AND_SINGLES"]),h.getAllTidalArtistAlbums(t,U,["COMPILATIONS"])])}async function tt(){if(e.artistId){const t=await h.getArtist(e.artistId);return f(t),t}else if(e.tidalArtistId){const t=await h.getArtistFromTidalArtistId(e.tidalArtistId);return f(t),t.qobuzId&&S(t.qobuzId),t}else if(e.qobuzArtistId){const t=await h.getArtistFromQobuzArtistId(e.qobuzArtistId);return f(t),t.tidalId&&P(t.tidalId),t}}async function lt(t){const a=await h.getTidalArtist(t);return R(a),a}async function Q(t){const a=await h.getQobuzArtist(t);return M(a),a}async function j(){const t=[];let a=!1;if(e.artistId){const $=await tt();a=!0,$?.tidalId&&t.push(P($.tidalId)),$?.qobuzId&&t.push(S($.qobuzId))}e.tidalArtistId&&t.push(lt(e.tidalArtistId)),e.qobuzArtistId&&t.push(Q(e.qobuzArtistId)),a||t.push(tt()),await Promise.all(t)}async function r(){try{if(e.artistId){const t=await h.getAllAlbums({artistId:e.artistId,sort:"Release-Date-Desc"});I(t)}else if(e.tidalArtistId){const t=await h.getAllAlbums({tidalArtistId:e.tidalArtistId,sort:"Release-Date-Desc"});I(t)}else if(e.qobuzArtistId){const t=await h.getAllAlbums({qobuzArtistId:e.qobuzArtistId,sort:"Release-Date-Desc"});I(t)}}catch{I(null)}}async function s(){e.artistId&&await r(),e.tidalArtistId&&await Promise.all([r(),P(e.tidalArtistId)]),e.qobuzArtistId&&await Promise.all([r(),S(e.qobuzArtistId)])}ht(gt(()=>e.artistId,(t,a)=>{t!==a&&l()})),ht(gt(()=>e.tidalArtistId,(t,a)=>{t!==a&&l()})),ht(gt(()=>e.qobuzArtistId,(t,a)=>{t!==a&&l()}));async function l(){await Promise.all([j(),s()])}return(()=>{var t=c(Wt),a=t.firstChild,$=a.firstChild,E=$.firstChild,q=$.nextSibling,G=q.firstChild,z=G.firstChild,L=z.nextSibling,A=L.firstChild,p=q.nextSibling,[_,nt]=m(p.nextSibling),ot=_.nextSibling,[H,ct]=m(ot.nextSibling),ut=H.nextSibling,[et,dt]=m(ut.nextSibling),rt=et.nextSibling,[N,zt]=m(rt.nextSibling),xt=N.nextSibling,[vt,Ct]=m(xt.nextSibling),kt=vt.nextSibling,[St,Tt]=m(kt.nextSibling),Pt=St.nextSibling,[Qt,Et]=m(Pt.nextSibling);return E.$$click=()=>Ft(),n(z,i(v,{get when(){return Z()},children:o=>i(It,{get artist(){return o()},route:!1,size:400})})),n(A,()=>Z()?.title),n(a,i(v,{get when(){return(V()?.length??0)>0},get children(){return[c(jt),(()=>{var o=c(C);return n(o,i(w,{get each(){return V()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),_,nt),n(a,i(v,{get when(){return(J()?.length??0)>0},get children(){return[c(Gt),(()=>{var o=c(C);return n(o,i(w,{get each(){return J()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),H,ct),n(a,i(v,{get when(){return(B()?.length??0)>0},get children(){return[c(Ht),(()=>{var o=c(C);return n(o,i(w,{get each(){return B()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),et,dt),n(a,i(v,{get when(){return(T()?.length??0)>0},get children(){return[c(Vt),(()=>{var o=c(C);return n(o,i(w,{get each(){return T()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),N,zt),n(a,i(v,{get when(){return(W()?.length??0)>0},get children(){return[c(Jt),(()=>{var o=c(C);return n(o,i(w,{get each(){return W()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),vt,Ct),n(a,i(v,{get when(){return(O()?.length??0)>0},get children(){return[c(Kt),(()=>{var o=c(C);return n(o,i(w,{get each(){return O()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),St,Tt),n(a,i(v,{get when(){return(Y()?.length??0)>0},get children(){return[c(Ut),(()=>{var o=c(C);return n(o,i(w,{get each(){return Y()},children:b=>i(x,{album:b,artist:!0,title:!0,year:!0,controls:!0,versionQualities:!0,size:200})})),o})()]}}),Qt,Et),it(),t})()}yt(["click"]);var Xt=d('<div class="artists-back-to-top-container main-content-back-to-top"><div class=artists-back-to-top><div class=artists-back-to-top-content><img class=artists-back-to-top-chevron src=/img/chevron-up-white.svg>Back to top<img class=artists-back-to-top-chevron src=/img/chevron-up-white.svg>'),Yt=d("<header class=artists-header-container><div class=artists-header-backdrop></div><div class=artists-header-text-container><h1 class=artists-header-text>Artists <img class=artists-header-sort-icon src=/img/more-options-white.svg></h1><!$><!/></div><input class=filter-artists type=text placeholder=Filter...>"),Zt=d("<div class=artists-page>"),te=d("<div class=artists-sort-controls><div>Artist Name<!$><!/><!$><!/>"),ee=d("<img class=sort-chevron-icon src=/img/chevron-up-white.svg>"),re=d("<img class=sort-chevron-icon src=/img/chevron-down-white.svg>"),ie=d("<div><p class=artists-header-artist-count>Showing <!$><!/> artist<!$><!/></p><div class=artists>");let D;function se(){let e,y,f;const[V,I]=g(!1),[F,R]=g(),[J,k]=g(),[B,K]=g("Name"),[T,U]=g(!1),u=new wt(window.location.search);qt(()=>{u.has("sort")&&K(u.get("sort"))});function M(r,s){u.set(r,s);const l=`${window.location.pathname}?${u}`;history.pushState(null,"",l),r==="sort"&&K(s)}function W(r){u.delete(r);const s=`${window.location.pathname}?${u}`;history.pushState(null,"",s)}function st(){return u.get("sources")?.split(",")}function O(){return u.get("sort")}function X(){return u.get("search")}function Y(r){M("sources",r.join(","))}function at(r){M("sort",r)}function Z(r){r.trim().length===0?W("search"):M("search",r),k(r)}async function S(r=void 0){const s=u.toString();if(!F()){const t=Mt();if(t&&t.query===s){R(t.results);return}}r?.sources&&Y(r.sources),r?.sort&&at(r.sort),typeof r?.filters?.search=="string"&&Z(r.filters.search);try{I(!0),R(await Rt("artists",t=>h.getArtists({sources:st(),sort:O(),filters:{search:X()}},t)))}catch(t){console.error("Failed to fetch artists",t),R(void 0)}finally{I(!1)}const l=F();l&&Ot({query:s,results:l})}D&&window.removeEventListener("popstate",D),D=()=>{const r=new wt(window.location.search);let s=!1;u.forEach((l,t)=>{if(!r.has(t))switch(t){case"sources":s=!0;break;case"sort":s=!0;break;case"search":u.delete(t),k(""),s=!0;break}}),r.forEach((l,t)=>{if(u.get(t)!==l)switch(u.set(t,l),t){case"sources":s=!0;break;case"sort":s=!0;break;case"search":k(l),s=!0;break}}),s&&S()},window.addEventListener("popstate",D),bt(()=>{D&&window.removeEventListener("popstate",D)});const P=r=>{T()&&U(!1)};mt(()=>{window.addEventListener("click",P)}),bt(()=>{window.removeEventListener("click",P)});function tt(){e.style.display!=="block"&&(clearTimeout(Q),e.style.opacity="0",e.style.display="block",Q=setTimeout(()=>{e.style.opacity="1"},0))}function lt(){e.style.opacity!=="0"&&(clearTimeout(Q),e.style.opacity="0",Q=setTimeout(()=>{e.style.display="none"},300))}let Q;const j=()=>{(document.querySelector("main")?.scrollTop??0)>f.getBoundingClientRect().bottom?tt():lt()};return mt(()=>{document.querySelector("main")?.addEventListener("scroll",j),j()}),bt(()=>{document.querySelector("main")?.removeEventListener("scroll",j)}),mt(async()=>{k(X()??""),await S()}),[(()=>{var r=c(Xt),s=r.firstChild,l=e;return typeof l=="function"?ft(l,s):e=s,s.$$click=()=>document.querySelector("main")?.scroll({top:0,behavior:"smooth"}),it(),r})(),(()=>{var r=c(Yt),s=r.firstChild,l=s.nextSibling,t=l.firstChild,a=t.firstChild,$=a.nextSibling,E=t.nextSibling,[q,G]=m(E.nextSibling),z=l.nextSibling,L=f;return typeof L=="function"?ft(L,r):f=r,$.$$click=A=>{U(!T()),A.stopPropagation()},n(l,(()=>{var A=$t(()=>!!T());return()=>A()&&(()=>{var p=c(te),_=p.firstChild,nt=_.firstChild,ot=nt.nextSibling,[H,ct]=m(ot.nextSibling),ut=H.nextSibling,[et,dt]=m(ut.nextSibling),rt=y;return typeof rt=="function"?ft(rt,p):y=p,_.$$click=()=>S({sort:O()==="Name-Desc"?"Name":"Name-Desc"}),n(_,(()=>{var N=$t(()=>B()==="Name");return()=>N()&&c(ee)})(),H,ct),n(_,(()=>{var N=$t(()=>B()==="Name-Desc");return()=>N()&&c(re)})(),et,dt),it(),p})()})(),q,G),Lt(z,"input",Bt(async A=>{await S({filters:{search:A.target.value??void 0}}),document.querySelector("main")?.scroll({top:0,behavior:"instant"})},200)),pt(()=>Nt(z,"value",J()??"")),it(),r})(),(()=>{var r=c(Zt);return n(r,i(v,{get when(){return F()},children:s=>(()=>{var l=c(ie),t=l.firstChild,a=t.firstChild,$=a.nextSibling,[E,q]=m($.nextSibling),G=E.nextSibling,z=G.nextSibling,[L,A]=m(z.nextSibling),p=t.nextSibling;return n(t,()=>s()?.length,E,q),n(t,()=>s()?.length===1?"":"s",L,A),n(p,i(w,{get each(){return s()},children:_=>i(It,{artist:_,size:200,title:!0})})),pt(()=>Dt(l,`artists-container${V()?" loading":" loaded"}`)),l})()})),r})()]}yt(["click","input"]);function de(){const e=At("artistId"),y=At("tidalArtistId"),f=At("qobuzArtistId");return e?i(_t,{get artistId(){return parseInt(e)}}):y?i(_t,{get tidalArtistId(){return parseInt(y)}}):f?i(_t,{get qobuzArtistId(){return parseInt(f)}}):i(se,{})}export{de as default};
//# sourceMappingURL=artists.Dhj29Wh4.js.map