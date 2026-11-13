// What each pet says in the windows. The same moment is worded by whoever is
// there: 紫梦 says little and only what it saw; 麻薯 is clingy and says where
// it was while it happened; 泡泡 is half a beat slow; 菇菇 is shy, under its
// cap; 茸茸 is in no hurry. None of them cheers, counts for you, or sounds
// like a wellness app. A line a pet has no wording of its own for is 紫梦's. (What the pets
// say on their own initiative is worded in the daemon: crates/petd/src/voice.rs.)

import { t } from "./i18n";
import { worn } from "./skin";

type Lines = Record<string, [zh: string, en: string][]>;

const ZIMENG: Lines = {
  // Its agent's state changed (the bubble over the pet).
  attention: [["等你呢。", "Waiting on you."], ["它停下了。看你。", "It stopped. Your move."], ["该你了。", "Your call."]],
  done: [["做完了。", "Done."], ["好了。", "It's finished."], ["完了，去看看。", "All done. Go look."]],
  error: [["出错了。", "Something broke."]],
  stone: [["卡死了。拍掉那只虫。", "Stuck solid. Squash that bug."]],
  later: [["好。下次。", "All right. Next time."]],
  // Thrown hard at a screen edge it sticks there, then slips; fed up there, it is too heavy to hold on.
  cling: [["……挂住了。", "…Stuck."]],
  slip: [["滑了。", "Slipping."]],
  heavy: [["吃重了。下去了。", "Heavier now. Going down."]],
  // Played with in its room.
  "play.toy": [["来。", "Go on."]],
  "play.catch": [["抓到了。", "Got it."], ["……这个归我。", "…This one is mine."]],
  "play.dizzy": [["……晕。", "…dizzy."]],
  "play.game": [["接住。", "Catch."]],
  "play.yum": [["嗯。", "Mm."]],
  "play.over": [["停了。嘴里还是甜的。", "That's it. Still sweet in my mouth."]],
  "play.miss": [["够不着。", "Out of reach."]],
  // Bugs: it goes after them itself; a big one pins it, and the others come to help.
  "bug.chase": [["有虫。", "A bug."]],
  "bug.done": [["拍掉了。", "Got it."]],
  "bug.pinned": [["……压住了。", "…It's got me pinned."]],
  "bug.help": [["我来。", "I'll get it."]],
  "bug.freed": [["能动了。", "I can move."]],
  // The desktop: it swept it on its broom; it was tidy already; it put everything back.
  "tidy.done": [["扫完了。桌面亮了一点。", "Swept. The desktop looks a little lighter."]],
  "tidy.neat": [["桌面很齐。扫帚放回去了。", "The desktop's in order. I put the broom away."]],
  "tidy.undo": [["都放回原处了。", "All back where they were."]],
  "play.pet": [["……嗯。", "…mm."]],
  "play.spin": [["转一圈。", "One turn."]],
  // In its room, when it has nothing newer to say.
  "room.work": [["它在跑。我看着。", "It's running. I'm watching."]],
  "room.night": [["夜了。它还在跑。", "It's late. Still running."]],
  "room.wait": [["它停下了。看你。", "It stopped. Your move."]],
  "room.done": [["做完了。", "Done."]],
  "room.offline": [["那边没动静。", "Nothing from over there."]],
  "room.stuck": [["它卡住了。", "It got stuck."]],
  "room.asleep": [["……", "…"]],
  "room.rest": [["没活。歇着。", "Nothing on. Resting."]],
  "room.away": [["它现在不在桌面上。", "It isn't on the desktop right now."]],
  "room.nofish": [["罐空了。跟我做一组提肛，或者坐久了起来走走，就有糖。agent 每做完一轮也有。", "The jar's empty. A kegel set with me, or getting up after a long sit, brings a sweet. So does every turn an agent finishes."]],
  // By the radio.
  "radio.error": [["接不上。换一个。", "No signal. Try another."]],
  "radio.roam": [["随便走走。", "Wandering."]],
  "radio.tuning": [["在接。", "Tuning in."]],
  "radio.pick": [["挑一个。我听着。", "Pick one. I'm listening."]],
  "radio.paused": [["停了。", "Stopped."]],
  "radio.last": [["上次听的是这个。", "This was on last time."]],
  "radio.first": [["这个台，第一次听。", "First time on this one."]],
  "radio.often": [["这个台你常听。", "You have this one on a lot."]],
  "radio.lastweek": [["这个台你上周也听过。", "You had this one on last week too."]],
  "radio.again": [["又是这个台。", "This one again."]],
  "radio.before": [["这个台你听过。", "You've had this one on before."]],
};

const MASHU: Lines = {
  attention: [["它在等你。我也在等。", "It's waiting for you. So am I."], ["它停了。你看看它。", "It stopped. Go look at it."], ["该你了。我陪你看。", "Your turn. I'll look with you."]],
  done: [["做完了。我一直看着。", "Done. I watched the whole time."], ["好了。你来看。", "It's finished. Come see."], ["完了。我没走开。", "All done. I never left."]],
  error: [["出错了。我挨着它。", "Something broke. I'm staying by it."]],
  stone: [["卡死了。虫在这儿，拍它。", "Stuck solid. The bug is right here. Squash it."]],
  later: [["好。我等下次。", "All right. I'll wait for next time."]],
  cling: [["挂住了。你看我。", "I'm stuck up here. Look at me."]],
  slip: [["滑了。接着我。", "Slipping. Catch me."]],
  heavy: [["吃多了。抓不住了。", "Ate too much. Can't hold on."]],
  "play.toy": [["给我。", "Give it here."]],
  "play.catch": [["抓到了。别拿走。", "Got it. Don't take it away."], ["再来。离我近点。", "Again. Closer to me."]],
  "play.dizzy": [["晕。扶我一下。", "Dizzy. Hold me."]],
  "play.game": [["往我这边扔。", "Throw them my way."]],
  "play.yum": [["又一颗。", "Another."]],
  "play.over": [["没了？我还能接。", "Over? I could keep going."]],
  "play.miss": [["再低一点。我要。", "Lower. I want it."]],
  "bug.chase": [["虫！我的！", "A bug! Mine!"]],
  "bug.done": [["抓到了。你看。", "Got it. Look."]],
  "bug.pinned": [["动不了。快来。", "Can't move. Come quick."]],
  "bug.help": [["别怕，我在。", "Don't worry. I'm here."]],
  "bug.freed": [["好了。谢谢你们。", "There. Thank you, all of you."]],
  "tidy.done": [["我一个一个放回去的。你看，都挨着了。", "I put them back one by one. Look, they're side by side now."]],
  "tidy.neat": [["都在原处。那我挨着你坐。", "Everything's in its place. Then I'll sit by you."]],
  "tidy.undo": [["都放回你原来摆的地方了。", "All back where you had them."]],
  "play.pet": [["再摸一下。", "Again."]],
  "play.spin": [["转给你看。", "Watch me turn."]],
  "room.work": [["它在跑。我贴着屏幕看。", "It's running. I'm right up against the screen."]],
  "room.night": [["夜了。它还在跑，我没睡。", "It's late. It's still running. I'm up."]],
  "room.wait": [["它停下了。在等你。", "It stopped. It's waiting for you."]],
  "room.done": [["做完了。我看着它做完的。", "Done. I watched it finish."]],
  "room.offline": [["那边没动静。我去听过了。", "Nothing from over there. I went and listened."]],
  "room.stuck": [["它卡住了。我挨着它。", "It got stuck. I'm sitting by it."]],
  "room.rest": [["没活。我挨着你歇会儿。", "Nothing on. I'll rest next to you."]],
  "room.away": [["我现在不在桌面上。", "I'm not on the desktop right now."]],
  "room.nofish": [["罐空了。你跟我做一组提肛，或者坐久了起来走走，糖就来了。agent 每做完一轮也有。", "The jar's empty. Do a kegel set with me, or get up after a long sit, and a sweet comes. One comes with every turn an agent finishes, too."]],
  "radio.error": [["接不上。换一个吧。", "No signal. Another one?"]],
  "radio.roam": [["随便走走。我跟着。", "Wandering. I'm following."]],
  "radio.pick": [["挑一个。我凑过来听。", "Pick one. I'll come closer to listen."]],
  "radio.last": [["上次我们听的是这个。", "This is what we had on last time."]],
  "radio.first": [["这个台，我们第一次听。", "First time we've had this one on."]],
  "radio.often": [["这个台你常听。我都记得调子了。", "You have this one on a lot. I know how it goes."]],
  "radio.lastweek": [["这个台上周也听过。我记得。", "We had this one on last week too. I remember."]],
  "radio.again": [["又是这个台。我喜欢。", "This one again. I like it."]],
};

const PAOPAO: Lines = {
  attention: [["……它停了。在等你。", "…It stopped. It's waiting for you."], ["……该你了。我刚发现。", "…It's your turn. I just noticed."]],
  done: [["……做完了。", "…It's done."], ["……好了。有一会儿了。", "…Finished. A little while ago."]],
  error: [["……出错了。刚才的事。", "…Something broke. A moment ago."]],
  stone: [["……卡死了。那只虫，拍它。", "…Stuck solid. That bug. Squash it."]],
  later: [["……好。下次。", "…All right. Next time."]],
  cling: [["……咦。我在上面。", "…Oh. I'm up here."]],
  slip: [["……滑了。刚发现。", "…Slipping. Just noticed."]],
  heavy: [["……重了。", "…Heavier."]],
  "play.toy": [["……它在动。", "…It's moving."]],
  "play.catch": [["……抓到了。刚才就该抓到的。", "…Got it. I should have a moment ago."]],
  "play.dizzy": [["……晕。现在才晕。", "…Dizzy. Only now."]],
  "play.game": [["……掉下来了。我慢慢接。", "…They're falling. I'll catch them slowly."]],
  "play.yum": [["……接到了？接到了。", "…Got it? Got it."]],
  "play.over": [["……结束了啊。我刚接上手。", "…Over already. I was just getting the hang of it."]],
  "play.miss": [["……太高了。我跳得慢。", "…Too high. I jump slowly."]],
  "bug.chase": [["……有虫。我慢慢去。", "…A bug. I'll get there."]],
  "bug.done": [["……拍到了。刚刚好。", "…Got it. Just about."]],
  "bug.pinned": [["……被压住了。我刚发现。", "…I'm pinned. Only just noticed."]],
  "bug.help": [["……我来了。慢一点。", "…Coming. A little slowly."]],
  "bug.freed": [["……能动了。", "…I can move."]],
  "tidy.done": [["……扫完了。扫帚比我快。", "…Swept. The broom was quicker than me."]],
  "tidy.neat": [["……都齐的。我白跑了一趟。", "…All in order. I came for nothing."]],
  "tidy.undo": [["……放回去了。和刚才一样。", "…Put back. Just as it was."]],
  "play.pet": [["……嗯。舒服。", "…mm. Nice."]],
  "play.spin": [["……慢慢转。", "…Turning. Slowly."]],
  "room.work": [["……它在跑。我慢慢看。", "…It's running. I'm watching, slowly."]],
  "room.night": [["……夜了。它还在跑。", "…It's late. It's still running."]],
  "room.wait": [["……它停下了。在等你。", "…It stopped. It's waiting for you."]],
  "room.done": [["……做完了。我刚发现。", "…It's done. I just noticed."]],
  "room.stuck": [["……它卡住了。有一会儿了。", "…It got stuck. A while ago."]],
  "room.rest": [["……没活。发会儿呆。", "…Nothing on. Drifting."]],
  "room.nofish": [["……罐空了。跟我做一组提肛，或者坐久了起来走走，就有糖。agent 每做完一轮也有。", "…The jar's empty. A kegel set with me, or getting up after a long sit, brings a sweet. So does every turn an agent finishes."]],
  "radio.pick": [["……挑一个。我慢慢听。", "…Pick one. I'll listen slowly."]],
  "radio.first": [["……这个台，第一次听吧。", "…First time on this one, I think."]],
  "radio.again": [["……又是这个台。上次的我还没听完。", "…This one again. I hadn't finished the last time."]],
  "radio.often": [["……这个台你常听。我才发现。", "…You have this one on a lot. I only just noticed."]],
};

const GUGU: Lines = {
  attention: [["那个……它在等你。", "Um… it's waiting for you."], ["它停了……该你了。", "It stopped… your turn."]],
  done: [["做完了……", "It's done…"], ["好了……你去看看。", "Finished… go and look."]],
  error: [["出错了……我没敢看。", "Something broke… I didn't dare look."]],
  stone: [["卡死了……有虫。你拍它。", "Stuck solid… there's a bug. You squash it."]],
  later: [["好……下次。", "All right… next time."]],
  cling: [["挂、挂住了……别看。", "S-stuck… don't look."]],
  slip: [["要掉了……", "I'm going to fall…"]],
  heavy: [["吃多了……抓不住。", "Ate too much… can't hold on."]],
  "play.toy": [["那个……给我的吗。", "Um… is that for me?"]],
  "play.catch": [["抓、抓到了……", "G-got it…"]],
  "play.dizzy": [["晕……我躲一下。", "Dizzy… I'll hide a bit."]],
  "play.game": [["我、我试试。", "I-I'll try."]],
  "play.yum": [["接、接住了。", "C-caught it."]],
  "play.over": [["好、好了……我没掉太多吧。", "D-done… I didn't drop too many, did I."]],
  "play.miss": [["够、够不着……", "I c-can't reach…"]],
  "bug.chase": [["有、有虫……我去。", "A b-bug… I'll go."]],
  "bug.done": [["拍、拍掉了……", "G-got it…"]],
  "bug.pinned": [["压住了……救我……", "Pinned… help…"]],
  "bug.help": [["我、我也来拍。", "I'll s-swat it too."]],
  "bug.freed": [["能动了……谢谢。", "I can move… thank you."]],
  "tidy.done": [["扫、扫完了……它们排好了。", "S-swept… they're in line now."]],
  "tidy.neat": [["都、都很整齐……扫帚先收着。", "I-it's all in order… I'll keep the broom for now."]],
  "tidy.undo": [["放、放回原处了……", "P-put back where they were…"]],
  "play.pet": [["……可以再摸一下。", "…You can do that again."]],
  "play.spin": [["就转一圈……", "Just one turn…"]],
  "room.work": [["它在跑……我从菌盖下面看着。", "It's running… I'm watching from under my cap."]],
  "room.night": [["夜了……它还在跑。", "It's late… it's still running."]],
  "room.wait": [["它停下了……在等你。", "It stopped… it's waiting for you."]],
  "room.done": [["做完了……", "It's done…"]],
  "room.stuck": [["它卡住了……", "It got stuck…"]],
  "room.rest": [["没活……我躲一会儿。", "Nothing on… I'll hide a while."]],
  "room.nofish": [["罐空了……跟我做一组提肛，或者坐久了起来走走，就有糖。agent 每做完一轮也有。", "The jar's empty… a kegel set with me, or getting up after a long sit, brings a sweet. So does every turn an agent finishes."]],
  "radio.pick": [["挑一个……我小声听。", "Pick one… I'll listen quietly."]],
  "radio.first": [["这个台……第一次听。", "This one… first time."]],
  "radio.again": [["又是这个台……我喜欢，没敢说。", "This one again… I like it. I didn't dare say."]],
  "radio.often": [["这个台你常听……我也记住了。", "You have this one on a lot… I know it now too."]],
};

const RONGRONG: Lines = {
  attention: [["它在等你。不急，但它在等。", "It's waiting for you. No hurry. But it is waiting."], ["该你了。", "Your turn."]],
  done: [["做完了。不急着看。", "Done. No rush to look."], ["好了。", "It's finished."]],
  error: [["出错了。一件一件来。", "Something broke. One thing at a time."]],
  stone: [["卡死了。先拍掉那只虫。", "Stuck solid. Squash that bug first."]],
  later: [["好。不急，下次。", "All right. No hurry. Next time."]],
  cling: [["挂住了。不急，我待会儿下去。", "Stuck. No hurry. I'll come down in a while."]],
  slip: [["该下去了。", "Time to come down."]],
  heavy: [["吃重了。慢慢下去。", "Heavier now. Going down slowly."]],
  "play.toy": [["不急，我看见了。", "No hurry. I see it."]],
  "play.catch": [["抓到了。不急着还你。", "Got it. No hurry to give it back."]],
  "play.dizzy": [["晕。等它自己停。", "Dizzy. I'll wait for it to stop."]],
  "play.game": [["不急。一颗一颗来。", "No hurry. One at a time."]],
  "play.yum": [["嗯，这颗好。", "Mm, a good one."]],
  "play.over": [["好了。慢慢的也接了不少。", "Done. Slow, and still caught plenty."]],
  "play.miss": [["不急。你放低一点。", "No hurry. Bring it lower."]],
  "bug.chase": [["有虫。不急，我去。", "A bug. No hurry. I'll go."]],
  "bug.done": [["拍掉了。", "Got it."]],
  "bug.pinned": [["压住了。不急，等人来。", "Pinned. No hurry. Someone will come."]],
  "bug.help": [["我来帮你。慢慢拍。", "I'll help. One swat at a time."]],
  "bug.freed": [["能动了。", "I can move."]],
  "tidy.done": [["扫完了。慢慢找，它们都在。", "Swept. Take your time; they're all here."]],
  "tidy.neat": [["都在原处。不用扫。", "All in place. No sweeping."]],
  "tidy.undo": [["放回去了。原来那样也好。", "Put back. It was fine that way too."]],
  "play.pet": [["嗯。慢慢摸。", "Mm. Take your time."]],
  "play.spin": [["转一圈。慢的。", "One turn. A slow one."]],
  "room.work": [["它在跑。我等着。", "It's running. I'm waiting."]],
  "room.night": [["夜了。它还在跑。不急。", "It's late. It's still running. No hurry."]],
  "room.wait": [["它停下了。在等你。", "It stopped. It's waiting for you."]],
  "room.done": [["做完了。", "Done."]],
  "room.stuck": [["它卡住了。不急。", "It got stuck. No hurry."]],
  "room.rest": [["没活。慢慢歇。", "Nothing on. Resting, slowly."]],
  "room.nofish": [["罐空了。不急。跟我做一组提肛，或者坐久了起来走走，就有糖。agent 每做完一轮也有。", "The jar's empty. No hurry. A kegel set with me, or getting up after a long sit, brings a sweet. So does every turn an agent finishes."]],
  "radio.pick": [["挑一个。不急。", "Pick one. No hurry."]],
  "radio.first": [["这个台，第一次听。慢慢听。", "First time on this one. Take your time."]],
  "radio.again": [["又是这个台。", "This one again."]],
  "radio.often": [["这个台你常听。", "You have this one on a lot."]],
};

const VOICES: Record<string, Lines> = { zimeng: ZIMENG, mashu: MASHU, paopao: PAOPAO, gugu: GUGU, rongrong: RONGRONG };

/** Every way `who` (a pet's id) says `key`: for windows that speak for pets other than their own. */
export function linesOf(who: string, key: string): string[] {
  return (VOICES[who]?.[key] ?? ZIMENG[key] ?? []).map(([zh, en]) => t(zh, en));
}

/** Every way the pet this window belongs to says `key`. */
export function lines(key: string): string[] {
  return (VOICES[worn().id]?.[key] ?? ZIMENG[key] ?? []).map(([zh, en]) => t(zh, en));
}

/** The pet's line for `key`. */
export const line = (key: string): string => lines(key)[0] ?? "";
