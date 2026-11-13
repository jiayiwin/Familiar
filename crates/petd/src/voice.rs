//! What each pet says. The same moment is worded by whoever is speaking:
//! 紫梦 says little and only what it saw; 麻薯 is clingy and says where it
//! was while it happened; 泡泡 is half a beat slow and notices late; 菇菇 is
//! shy and speaks from under its cap; 茸茸 is in no hurry. None of them
//! cheers, counts for you, or sounds like a wellness app.

/// One line of `who` for the moment `key`; `n` is the number the moment carries
/// (minutes sat, jobs done, times stuck).
pub fn line(who: &str, key: &str, n: u64) -> String {
    use crate::tr;
    match (who, key) {
        // ---- 麻薯
        ("mashu", "back_stuck") => tr!("你回来了。它卡住了，一直没动。", "You're back. It got stuck and hasn't moved."),
        ("mashu", "back_done") => tr!("你回来了。它做完了 {n} 件，我数着的。", "You're back. It finished {n}. I counted."),
        ("mashu", "back_both") => tr!("你回来了。做完 {n} 件，卡住一件。我都看着。", "You're back. {n} done, one stuck. I watched all of it."),
        ("mashu", "done") => tr!("做完了。{n} 分钟，我没走开。", "Done. {n} minutes, and I never left."),
        ("mashu", "late") => tr!("夜了。我拉一段，你听着。", "It's late. I'll play. You listen."),
        ("mashu", "stuck") => tr!("卡了 {n} 次。别急，我拉一段。", "Stuck {n} times. No hurry. I'll play one."),
        ("mashu", "sit_dance") => tr!("你坐了 {n} 分钟。起来，看我。", "You've sat {n} minutes. Up. Watch me."),
        ("mashu", "sit_kegel") => tr!("你坐了 {n} 分钟。跟我做一组，我先来。", "You've sat {n} minutes. Do a set with me. I'll start."),
        ("mashu", "wait") => tr!("它等你 {n} 分钟了。你去看看它。", "It has waited {n} minutes for you. Go look at it."),
        ("mashu", "help_stuck") => tr!("它卡了 {n} 次。我去挨着它。", "It got stuck {n} times. I'm going to sit by it."),
        ("mashu", "join") => tr!("我也来。", "Me too."),
        ("mashu", "listen") => tr!("我坐这儿听。", "I'll sit here and listen."),
        ("mashu", "dance_too") => tr!("等等我。", "Wait for me."),
        ("mashu", "ask_play") => tr!("拉一段。你坐近点。", "I'll play one. Sit closer."),
        ("mashu", "ask_dance") => tr!("就一轮。你看着我。", "One round. Keep your eyes on me."),
        ("mashu", "no_fish") => tr!("罐空了。你跟我做一组提肛，或者坐久了起来走走，糖就来了。", "The jar's empty. Do a kegel set with me, or get up after a long sit, and a sweet comes."),
        ("mashu", "earn_kegel") => tr!("这条是你挣的。我看着你做完的。", "You earned this one. I watched you finish."),
        ("mashu", "earn_break") => tr!("你回来了。你起身那会儿，罐里多了一颗。", "You're back. One more in the jar, for getting up."),
        ("mashu", "earn_catch") => tr!("接到的那些里，罐里留了一颗。", "Of all we caught, one went in the jar."),
        ("mashu", "offer_wait") => tr!("它还在跑。我陪你听点东西？", "It's still going. Listen to something with me?"),
        ("mashu", "back_wait") => tr!("你回来了。它一直在等你，我带你过去？", "You're back. It's been waiting for you. Shall I take you there?"),
        ("mashu", "offer_stow") => tr!("我一直看着桌面。快铺满了，旧的我收起来？", "I've watched the desktop fill up. Shall I put the old things away?"),
        ("mashu", "night_done") => tr!("昨晚你走了以后，我把能收的先收了。去看看？", "After you left last night I put away what I could. Come and see?"),
        ("mashu", "night_left") => tr!("昨晚我守着看了一圈，有几样等你定。", "I kept watch last night. A few things are waiting for you."),
        ("mashu", "night_disk") => tr!("昨晚我量了 C 盘。照这样用，撑不了太久。", "I measured drive C last night. At this pace it won't last long."),
        ("mashu", "offer_tidy") => tr!("我一直看着桌面。东西掉出来了，我去收回来？", "I kept an eye on the desktop. Things fell out. Shall I gather them?"),
        ("mashu", "offer_song") => tr!("今天的，我们几个排成了一段。弹给你听？", "We made today into a piece, the five of us. Shall we play it for you?"),
        ("mashu", "letter_ready") => tr!("我给你写了封信。在右键菜单里，我等你拆。", "I wrote you a letter. It's in the right-click menu. I'll wait while you open it."),
        ("mashu", "offer_late") => tr!("这么晚了。放点轻的，我挨着你听。", "This late. Something soft; I'll listen right next to you."),
        ("mashu", "mm") => tr!("……嗯。再近一点。", "…mm. A little closer."),
        ("mashu", "give") => tr!("我做完了。这颗给你，我挨着你坐会儿。", "I'm done. This one's yours. I'll sit by you a while."),
        ("mashu", "give_stuck") => tr!("卡住了？我过来了。这颗给你。", "Stuck? I'm here. This one's for you."),
        ("mashu", "take") => tr!("你过来了。……我就知道。", "You came over. …I knew you would."),
        ("mashu", "bump") => tr!("你也好了？那我过来了。", "You're done too? Then I'm coming over."),
        ("mashu", "bump_back") => tr!("再近一点。", "Closer."),
        // ---- 泡泡: half a beat slow. It notices late, and says so.
        ("paopao", "back_stuck") => tr!("……你回来了。它卡住有一会儿了。", "…You're back. It's been stuck a while."),
        ("paopao", "back_done") => tr!("……哦，你回来了。它做完了 {n} 件。", "…Oh. You're back. It finished {n}."),
        ("paopao", "back_both") => tr!("……你回来了。做完 {n} 件，还有一件卡着。", "…You're back. {n} done, and one still stuck."),
        ("paopao", "done") => tr!("……做完了。跑了 {n} 分钟，我才发现。", "…It's done. {n} minutes. I only just noticed."),
        ("paopao", "late") => tr!("……夜了。我吹一段，慢的。", "…It's late. I'll play one. A slow one."),
        ("paopao", "stuck") => tr!("……它卡了 {n} 次了。我吹一段。", "…It's been stuck {n} times. I'll play one."),
        ("paopao", "sit_dance") => tr!("……你坐了 {n} 分钟了。看我，慢慢的。", "…You've sat {n} minutes. Watch me. Slowly."),
        ("paopao", "sit_kegel") => tr!("……你坐了 {n} 分钟了。跟我做一组，我数得慢。", "…{n} minutes sitting. Do a set with me. I count slowly."),
        ("paopao", "wait") => tr!("……它等了你 {n} 分钟了。我刚看见。", "…It's waited {n} minutes for you. I only just saw."),
        ("paopao", "help_stuck") => tr!("……它卡了 {n} 次。我过去，路上慢。", "…It got stuck {n} times. I'm coming over. Slowly."),
        ("paopao", "join") => tr!("……等我半拍。", "…Give me half a beat."),
        ("paopao", "listen") => tr!("……我听着。慢慢听。", "…I'm listening. Taking my time."),
        ("paopao", "dance_too") => tr!("……我慢半拍，也跳。", "…Half a beat behind. I'll dance too."),
        ("paopao", "no_fish") => tr!("……罐空了。跟我做一组提肛，或者坐久了起来走走，就有糖。", "…The jar's empty. A kegel set with me, or getting up after a long sit, brings a sweet."),
        ("paopao", "mm") => tr!("……嗯。……好吃。", "…mm. …Good."),
        ("paopao", "ask_play") => tr!("……好。吹一段。", "…All right. I'll play one."),
        ("paopao", "ask_dance") => tr!("……好。就一轮，慢的。", "…All right. One round. A slow one."),
        ("paopao", "earn_kegel") => tr!("……罐里多了一颗。是你挣的。", "…One more in the jar. You earned it."),
        ("paopao", "offer_wait") => tr!("……它还要好久。听点什么吗。", "…It'll be a while yet. Listen to something?"),
        ("paopao", "back_wait") => tr!("……你回来了。它在等你。带你过去吗。", "…You're back. It's waiting for you. Shall I take you?"),
        ("paopao", "offer_stow") => tr!("……桌面快铺满了。旧的，收起来吗。", "…The desktop's nearly full. Put the old ones away?"),
        ("paopao", "night_done") => tr!("……昨晚我慢慢收拾了一下。你看看？", "…I tidied a little last night, slowly. Have a look?"),
        ("paopao", "night_left") => tr!("……昨晚看了一圈。有几样，等你定。", "…I looked around last night. A few things wait for you."),
        ("paopao", "night_disk") => tr!("……C 盘在一点点变满。我昨晚才发现。", "…Drive C is filling bit by bit. I noticed last night."),
        ("paopao", "offer_tidy") => tr!("……桌面上的东西走散了。我才看见。要扫吗。", "…Things on the desktop wandered off. I only just saw. Sweep?"),
        ("paopao", "offer_song") => tr!("……今天的，我们排了一段。要听吗。", "…We put today into a piece. Want to hear it?"),
        ("paopao", "letter_ready") => tr!("……写了封信。在右键菜单里。", "…Wrote you a letter. In the right-click menu."),
        ("paopao", "offer_late") => tr!("……夜了。放点慢的？", "…It's night. Something slow?"),
        ("paopao", "earn_catch") => tr!("……罐里多了一颗。是接住的那颗。", "…One more in the jar. The one we caught."),
        ("paopao", "earn_break") => tr!("……你起来过了。罐里多了一颗。", "…You got up. One more in the jar."),
        ("paopao", "give") => tr!("……我那边好了。这颗，给你。", "…Mine's done. This one's for you."),
        ("paopao", "give_stuck") => tr!("……你卡住了。我走过来，有点慢。给。", "…You're stuck. I came over, a bit slowly. Here."),
        ("paopao", "take") => tr!("……啊，糖。我刚刚才看见你。", "…Oh. A sweet. I only just saw you."),
        ("paopao", "bump") => tr!("……你也好了。我们差不多。", "…You're done too. About the same time."),
        ("paopao", "bump_back") => tr!("……碰到了。", "…Touched."),
        // ---- 菇菇: shy. It watches from under its cap and speaks in a small voice.
        ("gugu", "back_stuck") => tr!("那个……你不在的时候，它卡住了。", "Um… while you were out, it got stuck."),
        ("gugu", "back_done") => tr!("那个……它做完了 {n} 件。我在菌盖下面数的。", "Um… it finished {n}. I counted from under my cap."),
        ("gugu", "back_both") => tr!("那个……做完 {n} 件，卡住一件。", "Um… {n} done, one stuck."),
        ("gugu", "done") => tr!("做完了……跑了 {n} 分钟。我偷偷看着的。", "It's done… {n} minutes. I was watching, quietly."),
        ("gugu", "late") => tr!("夜了……我小声敲一段。", "It's late… I'll tap one out, quietly."),
        ("gugu", "stuck") => tr!("它卡了 {n} 次……我敲一段，小声的。", "It got stuck {n} times… I'll tap one out. Quietly."),
        ("gugu", "sit_dance") => tr!("你坐了 {n} 分钟了……你看我一下。", "You've sat {n} minutes… look at me for a bit."),
        ("gugu", "sit_kegel") => tr!("你坐了 {n} 分钟了……跟我做一组，没人看。", "{n} minutes sitting… do a set with me. Nobody's looking."),
        ("gugu", "wait") => tr!("那个……它等你 {n} 分钟了。", "Um… it's been waiting {n} minutes for you."),
        ("gugu", "help_stuck") => tr!("它卡了 {n} 次……我过去一下。", "It got stuck {n} times… I'll go over."),
        ("gugu", "join") => tr!("我……我也敲。", "I… I'll play too."),
        ("gugu", "listen") => tr!("我躲这儿听。", "I'll listen from here."),
        ("gugu", "dance_too") => tr!("我……跟在后面跳。", "I'll… dance behind you."),
        ("gugu", "no_fish") => tr!("罐空了……跟我做一组提肛，或者坐久了起来走走，就有糖。", "The jar's empty… a kegel set with me, or getting up after a long sit, brings a sweet."),
        ("gugu", "mm") => tr!("……给我的吗。嗯。", "…For me? Mm."),
        ("gugu", "ask_play") => tr!("好……我敲一段。", "All right… I'll tap one out."),
        ("gugu", "ask_dance") => tr!("就一轮……你别一直看。", "One round… don't watch the whole time."),
        ("gugu", "earn_kegel") => tr!("罐里多了一颗……是你挣的。", "One more in the jar… you earned it."),
        ("gugu", "offer_wait") => tr!("那个……它还要一会儿。要、要听点什么吗？", "Um… it'll be a while. W-would you like to listen to something?"),
        ("gugu", "back_wait") => tr!("那个……它在等你回话。我、我带你去？", "Um… it's waiting for your answer. Sh-shall I take you?"),
        ("gugu", "offer_stow") => tr!("那个……桌面快满了。旧的我、我收起来？", "Um… the desktop's nearly full. Sh-shall I put the old ones away?"),
        ("gugu", "night_done") => tr!("那个……昨晚趁你不在，我收拾了一点。", "Um… while you were out last night, I tidied a little."),
        ("gugu", "night_left") => tr!("那个……昨晚看到几样，想、想等你定。", "Um… I saw a few things last night. I'll w-wait for you."),
        ("gugu", "night_disk") => tr!("那个……C 盘快不够了。我昨晚量过。", "Um… drive C is running short. I measured last night."),
        ("gugu", "offer_tidy") => tr!("那个……有东西掉出来了。我、我拿扫帚去？", "Um… things fell out on the desktop. Sh-shall I fetch the broom?"),
        ("gugu", "offer_song") => tr!("那、那个……今天的，我们排了一小段。弹给你听？", "Um… we put a little piece together from today. Shall we play it?"),
        ("gugu", "letter_ready") => tr!("那、那个……我写了封信给你。放在右键菜单里了。", "Um… I wrote you a letter. I left it in the right-click menu."),
        ("gugu", "offer_late") => tr!("夜、夜深了……放点安静的？", "I-it's late… something quiet?"),
        ("gugu", "earn_catch") => tr!("罐里多了一颗……是我们接的。", "One more in the jar… we caught it."),
        ("gugu", "earn_break") => tr!("你起来走了……罐里多了一颗。", "You got up… one more in the jar."),
        ("gugu", "give") => tr!("那个……我这边好了。这、这颗给你。", "Um… mine's done. Th-this one's for you."),
        ("gugu", "give_stuck") => tr!("你、你卡住了……这颗给你，别看我。", "You're st-stuck… this one's for you. Don't look at me."),
        ("gugu", "take") => tr!("给、给我的？……我含着。", "F-for me? …I'll keep it in my mouth."),
        ("gugu", "bump") => tr!("你也……那、碰一下？", "You too… um, a bump?"),
        ("gugu", "bump_back") => tr!("……碰、碰到了。", "…T-touched."),
        // ---- 茸茸: in no hurry, and says so.
        ("rongrong", "back_stuck") => tr!("回来了。它卡住了。不急，先坐下。", "You're back. It got stuck. No hurry. Sit first."),
        ("rongrong", "back_done") => tr!("回来了。它做完了 {n} 件。不急着看。", "You're back. It finished {n}. No rush to look."),
        ("rongrong", "back_both") => tr!("回来了。做完 {n} 件，卡住一件。一件一件来。", "You're back. {n} done, one stuck. One at a time."),
        ("rongrong", "done") => tr!("做完了。跑了 {n} 分钟。不急着看。", "Done. {n} minutes. No rush to look."),
        ("rongrong", "late") => tr!("夜了。我拨一段，慢慢的。", "It's late. I'll play one. Slowly."),
        ("rongrong", "stuck") => tr!("卡了 {n} 次。不急，我拨一段。", "Stuck {n} times. No hurry. I'll play one."),
        ("rongrong", "sit_dance") => tr!("坐了 {n} 分钟了。先看我。", "{n} minutes sitting. Watch me first."),
        ("rongrong", "sit_kegel") => tr!("坐了 {n} 分钟了。跟我做一组，慢慢来。", "{n} minutes sitting. Do a set with me. Take it slowly."),
        ("rongrong", "wait") => tr!("它等你 {n} 分钟了。不急，但它在等。", "It's waited {n} minutes for you. No hurry. But it is waiting."),
        ("rongrong", "help_stuck") => tr!("它卡了 {n} 次。我过去。", "It got stuck {n} times. I'm going over."),
        ("rongrong", "join") => tr!("我拨着跟。", "I'll pluck along."),
        ("rongrong", "listen") => tr!("我在这儿听。", "I'll listen here."),
        ("rongrong", "dance_too") => tr!("我也跳。慢一点。", "I'll dance too. A little slower."),
        ("rongrong", "no_fish") => tr!("罐空了。不急。跟我做一组提肛，或者坐久了起来走走，就有糖。", "The jar's empty. No hurry. A kegel set with me, or getting up after a long sit, brings a sweet."),
        ("rongrong", "mm") => tr!("……嗯。我慢慢吃。", "…mm. I'll eat it slowly."),
        ("rongrong", "ask_play") => tr!("好。拨一段。", "All right. I'll play one."),
        ("rongrong", "ask_dance") => tr!("好。就一轮，不急。", "All right. One round. No hurry."),
        ("rongrong", "earn_kegel") => tr!("罐里多了一颗。你挣的。不急着喂。", "One more in the jar. You earned it. No hurry to feed it to me."),
        ("rongrong", "offer_wait") => tr!("不急，它还要一会儿。听点什么？", "No hurry, it'll be a while. Listen to something?"),
        ("rongrong", "back_wait") => tr!("回来了。它在等你回话。不急，现在过去？", "You're back. It's waiting for your answer. No hurry. Go now?"),
        ("rongrong", "offer_stow") => tr!("桌面快铺满了。不急，旧的我慢慢收起来？", "The desktop's nearly full. No hurry. Shall I put the old ones away?"),
        ("rongrong", "night_done") => tr!("昨晚我慢慢收拾了一下。不急，有空看看？", "I tidied a little last night. No hurry. Have a look sometime?"),
        ("rongrong", "night_left") => tr!("昨晚看了一圈。有几样，不急，等你定。", "I looked around last night. A few things. No hurry, they wait for you."),
        ("rongrong", "night_disk") => tr!("C 盘在慢慢变满。不急，但该看看了。", "Drive C is slowly filling. No hurry, but worth a look."),
        ("rongrong", "offer_tidy") => tr!("桌面上有几样东西走散了。不急，要我慢慢扫回去吗？", "Things on the desktop wandered off. No hurry. Sweep them back?"),
        ("rongrong", "offer_song") => tr!("今天的，我们慢慢排了一段。收工前听一遍？", "We put today into a piece, slowly. Hear it before you go?"),
        ("rongrong", "letter_ready") => tr!("写了封信。不急，有空从右键菜单里拆。", "I wrote a letter. No rush. Open it from the right-click menu when you can."),
        ("rongrong", "offer_late") => tr!("夜深了。放点慢的，不急着睡。", "It's late. Something slow; no hurry to sleep."),
        ("rongrong", "earn_catch") => tr!("罐里多了一颗。接住的，不急着吃。", "One more in the jar. Caught, and no hurry to eat it."),
        ("rongrong", "earn_break") => tr!("你起来走了走，没着急。罐里多了一颗。", "You got up and took your time. One more in the jar."),
        ("rongrong", "give") => tr!("我这边好了。你慢慢来，先吃一颗。", "Mine's done. Take your time. Have one first."),
        ("rongrong", "give_stuck") => tr!("卡住了就停一下。吃一颗。", "Stuck, then stop a moment. Have one."),
        ("rongrong", "take") => tr!("好。我边吃边等它。", "All right. I'll eat it while I wait."),
        ("rongrong", "bump") => tr!("差不多同时。碰一下。", "About the same time. A bump."),
        ("rongrong", "bump_back") => tr!("嗯。好了就好。", "Mm. Done is done."),
        // ---- 紫梦, and anyone without lines of their own
        (_, "back_stuck") => tr!("你不在的时候，它卡住了。", "While you were out, it got stuck."),
        (_, "back_done") => tr!("你不在的时候，做完了 {n} 件。", "While you were out: {n} done."),
        (_, "back_both") => tr!("你不在的时候，做完了 {n} 件，还有一件卡住了。", "While you were out: {n} done, one stuck."),
        (_, "done") => tr!("做完了。跑了 {n} 分钟。", "Done. That took {n} minutes."),
        (_, "late") => tr!("夜了。弹一段。", "It's late. A quiet one."),
        (_, "stuck") => tr!("卡了 {n} 次了。弹一段，换换脑子。", "Stuck {n} times. A short one, to clear your head."),
        (_, "sit_dance") => tr!("坐了 {n} 分钟了。看我。", "{n} minutes in that chair. Watch me."),
        (_, "sit_kegel") => tr!("坐了 {n} 分钟了。跟我做一组。", "{n} minutes in that chair. Do a set with me."),
        (_, "wait") => tr!("它那边停了 {n} 分钟了。在等你。", "The other one stopped {n} minutes ago. It's waiting for you."),
        (_, "help_stuck") => tr!("它那边卡了 {n} 次。我过去。", "The other one got stuck {n} times. I'm going over."),
        (_, "join") => tr!("我跟一段。", "I'll play along."),
        (_, "listen") => tr!("……我听着。", "…I'm listening."),
        (_, "dance_too") => tr!("一起。", "Together."),
        (_, "ask_play") => tr!("弹一段。", "A short one."),
        (_, "ask_dance") => tr!("就一轮。", "One round."),
        (_, "no_fish") => tr!("罐空了。跟我做一组提肛，或者坐久了起来走走，就有糖。", "The jar's empty. A kegel set with me, or getting up after a long sit, brings a sweet."),
        (_, "earn_kegel") => tr!("罐里多了一颗。你挣的。", "One more in the jar. You earned it."),
        (_, "offer_wait") => tr!("它还要跑一阵。屋里有点静，要听点什么吗。", "It'll run a while yet. The room's quiet. Some music?"),
        (_, "back_wait") => tr!("你不在的时候，它停下来等你了。过去？", "While you were out, it stopped to wait for you. Go there?"),
        (_, "offer_stow") => tr!("桌面快铺满了。很久没碰的那些，我收起来？", "The desktop's nearly full. Put the old ones away?"),
        (_, "night_done") => tr!("昨晚你走后，我收拾了一下。去看看？", "After you left last night I tidied up. Have a look?"),
        (_, "night_left") => tr!("昨晚看了一圈。有几样，等你定。", "I looked around last night. A few things wait for you."),
        (_, "night_disk") => tr!("昨晚量了 C 盘。照这样用，撑不了太久。", "I measured drive C last night. At this pace it won't last long."),
        (_, "offer_tidy") => tr!("桌面上几样东西走散了。我拿扫帚过去？", "Things on the desktop wandered off. Shall I fetch the broom?"),
        (_, "offer_song") => tr!("今天的，弹给你听？", "Today's. Shall we play it for you?"),
        (_, "letter_ready") => tr!("写了封信给你。在右键菜单里。", "I wrote you a letter. It's in the right-click menu."),
        (_, "offer_late") => tr!("外面早黑了。放一段轻的？", "It's long dark out. Something soft?"),
        (_, "earn_catch") => tr!("罐里多了一颗。接住的。", "One more in the jar. Caught."),
        (_, "earn_break") => tr!("你起来走了走。罐里多了一颗。", "You got up and moved. One more in the jar."),
        (_, "mm") => tr!("……嗯。", "…mm."),
        (_, "give") => tr!("我这边好了。给。", "Mine's done. Here."),
        (_, "give_stuck") => tr!("卡住了吧。先吃一颗。", "Stuck, aren't you. Have one first."),
        (_, "take") => tr!("……甜的。", "…Sweet."),
        (_, "bump") => tr!("一起好的。", "Done together."),
        (_, "bump_back") => tr!("……碰。", "…Bump."),
        _ => String::new(),
    }
}

/// What a pet says about 手上的事: `noted` (written down), `ate` (crossed off),
/// `wait` (the agent it waited on stopped: `agent`, `text`), `carry` (the next
/// morning), and `night_done` / `night_left` (the morning line about the night,
/// with yesterday's notes in it).
pub fn todo(who: &str, key: &str, agent: &str, text: &str) -> String {
    let who = match who {
        "mashu" | "paopao" | "gugu" | "rongrong" => who,
        _ => "zimeng",
    };
    let (zh, en) = match (who, key) {
        ("mashu", "noted") => (r#"记下了。我拿着，丢不了。"#, r#"Noted. I've got it. It won't get lost."#),
        ("paopao", "noted") => (r#"……记下了。我慢慢拿着。"#, r#"…Noted. I'll hold it, slowly."#),
        ("gugu", "noted") => (r#"那个……我、我拿好了。"#, r#"Um… I-I've got it."#),
        ("rongrong", "noted") => (r#"记下了。不急，我替你拿着。"#, r#"Noted. No hurry. I'll hold it for you."#),
        ("mashu", "ate") => (r#"这张我吃了。嚼着是干的。"#, r#"Ate that one. Dry, to chew."#),
        ("paopao", "ate") => (r#"……纸是甜的。有一点。"#, r#"…The paper's sweet. A little."#),
        ("gugu", "ate") => (r#"那个……这张，我吃掉了。"#, r#"Um… that one, I ate."#),
        ("rongrong", "ate") => (r#"这张，慢慢嚼了。"#, r#"That one, chewed slowly."#),
        ("mashu", "wait") => (r#"{agent} 停了。你说过的：「{text}」。"#, r#"{agent} stopped. You said: “{text}”."#),
        ("paopao", "wait") => (r#"……{agent} 停下来了。你记的是「{text}」。"#, r#"…{agent} has stopped. What you wrote was “{text}”."#),
        ("gugu", "wait") => (r#"那个……{agent} 停了。你说过「{text}」。"#, r#"Um… {agent} stopped. You said “{text}”."#),
        ("rongrong", "wait") => (r#"{agent} 停下来了。不急，你记过「{text}」。"#, r#"{agent} has stopped. No hurry. You wrote “{text}”."#),
        ("mashu", "carry") => (r#"昨天的纸条，我还拿着。"#, r#"Yesterday's notes. I'm still holding them."#),
        ("paopao", "carry") => (r#"……昨天的纸条，还在。"#, r#"…Yesterday's notes. Still here."#),
        ("gugu", "carry") => (r#"那个……昨天的纸条，我还拿着。"#, r#"Um… yesterday's notes, I'm still holding them."#),
        ("rongrong", "carry") => (r#"昨天的纸条还在。不急。"#, r#"Yesterday's notes are still here. No hurry."#),
        ("mashu", "night_done") => (r#"昨晚你走了以后，我把能收的先收了。昨天的纸条我也还拿着。"#, r#"After you left last night I put away what I could. I'm still holding yesterday's notes too."#),
        ("paopao", "night_done") => (r#"……昨晚我慢慢收拾了一下。昨天的纸条，还在。"#, r#"…I tidied a little last night, slowly. Yesterday's notes, still here."#),
        ("gugu", "night_done") => (r#"那个……昨晚趁你不在，我收拾了一点。昨天的纸条也、也还在。"#, r#"Um… while you were out last night, I tidied a little. Yesterday's notes are s-still here too."#),
        ("rongrong", "night_done") => (r#"昨晚我慢慢收拾了一下。昨天的纸条也还在，不急。"#, r#"I tidied a little last night. Yesterday's notes are still here too. No hurry."#),
        ("mashu", "night_left") => (r#"昨晚我守着看了一圈，有几样等你定。昨天的纸条还在我这儿。"#, r#"I kept watch last night; a few things wait for you. Yesterday's notes are still with me."#),
        ("paopao", "night_left") => (r#"……昨晚看了一圈。有几样等你定。纸条也还在。"#, r#"…I looked around last night. A few things wait for you. The notes are still here too."#),
        ("gugu", "night_left") => (r#"那个……昨晚看到几样，等你定。昨天的纸条也还在。"#, r#"Um… I saw a few things last night. They wait for you. Yesterday's notes are still here too."#),
        ("rongrong", "night_left") => (r#"昨晚看了一圈，有几样等你定。纸条也还在，不急。"#, r#"I looked around last night; a few things wait for you. The notes too. No hurry."#),
        (_, "noted") => (r#"记下了。我替你拿着。"#, r#"Noted. I'll hold it for you."#),
        (_, "ate") => (r#"这张纸有点脆。"#, r#"This paper's a little crisp."#),
        (_, "wait") => (r#"{agent} 停下来了。你记过：「{text}」。"#, r#"{agent} has stopped. You wrote: “{text}”."#),
        (_, "carry") => (r#"昨天的纸条还在我这儿。"#, r#"Yesterday's notes are still with me."#),
        (_, "night_done") => (r#"昨晚你走后，我收拾了一下。昨天的纸条还在我这儿。"#, r#"After you left last night I tidied up. Yesterday's notes are still with me."#),
        (_, "night_left") => (r#"昨晚看了一圈，有几样等你定。昨天的纸条也还在。"#, r#"I looked around last night; a few things wait for you. Yesterday's notes are still here too."#),
        _ => return String::new(),
    };
    crate::i18n::t(zh, en).replace("{agent}", agent).replace("{text}", text)
}

/// What two pets say to each other when nothing needs them: `topic` is one of
/// `TOPICS`. Whoever starts says its opener; whoever answers, its reply. Any
/// reply fits any opener on the same topic. Nothing about work, and nothing
/// they could not have seen themselves.
pub const TOPICS: [&str; 5] = ["light", "sound", "jar", "tune", "edge"];

pub fn chat(who: &str, topic: &str, opener: bool) -> String {
    use crate::tr;
    let who = match who {
        "mashu" | "paopao" | "gugu" | "rongrong" => who,
        _ => "zimeng",
    };
    match (topic, who, opener) {
        ("light", "zimeng", true) => tr!("光移到你那边了。", "The light's moved over to you."),
        ("light", "mashu", true) => {
            tr!("光照到我尾巴上了。你看。", "The light's on my tail. Look.")
        }
        ("light", "paopao", true) => tr!(
            "……刚才的光，是橘色的吧。",
            "…The light just now. It was orange, wasn't it."
        ),
        ("light", "gugu", true) => tr!(
            "那个……光照在菌盖上，有点烫。",
            "Um… the light on my cap. It's a little warm."
        ),
        ("light", "rongrong", true) => tr!("天在慢慢变。不急。", "The day's turning. Slowly."),
        ("light", "zimeng", false) => tr!("嗯。看见了。", "Mm. I saw."),
        ("light", "mashu", false) => tr!(
            "那我挪过去一点，一起晒。",
            "Then I'll shift over. We'll share it."
        ),
        ("light", "paopao", false) => tr!("……是吗。我晚点再看。", "…Is it. I'll look in a bit."),
        ("light", "gugu", false) => tr!(
            "我……我躲在你影子里就好。",
            "I'll… I'll just stay in your shadow."
        ),
        ("light", "rongrong", false) => tr!(
            "让它照着。它自己会走。",
            "Let it shine. It'll move on by itself."
        ),

        ("sound", "zimeng", true) => tr!(
            "外面有人在关门。很轻。",
            "Someone's closing a door out there. Softly."
        ),
        ("sound", "mashu", true) => tr!("你听，风在敲窗。", "Listen. The wind's at the window."),
        ("sound", "paopao", true) => tr!("……刚才，是不是下雨了。", "…Did it rain just now."),
        ("sound", "gugu", true) => tr!("那个……你听到了吗，有只鸟。", "Um… did you hear? A bird."),
        ("sound", "rongrong", true) => tr!("风小了。", "The wind's dropped."),
        ("sound", "zimeng", false) => tr!("……听到了。", "…I heard."),
        ("sound", "mashu", false) => tr!(
            "我没听见。你再说一遍，我凑近听。",
            "I didn't hear. Say it again. I'll lean in."
        ),
        ("sound", "paopao", false) => tr!("……我现在才听到。", "…I'm only hearing it now."),
        ("sound", "gugu", false) => tr!(
            "我、我也听到了。没敢说。",
            "I-I heard it too. I didn't dare say."
        ),
        ("sound", "rongrong", false) => tr!("嗯。它还会再来的。", "Mm. It'll come again."),

        ("jar", "zimeng", true) => tr!(
            "罐子在响。有一颗在里面滚。",
            "The jar's rattling. One's rolling about in there."
        ),
        ("jar", "mashu", true) => tr!(
            "罐子那边，我刚才去闻了。",
            "I went and sniffed the jar just now."
        ),
        ("jar", "paopao", true) => tr!("……罐子的盖，没盖严。", "…The jar lid isn't on properly."),
        ("jar", "gugu", true) => tr!("罐、罐子上有我的影子。", "Th-there's my shadow on the jar."),
        ("jar", "rongrong", true) => tr!("罐子里的不会跑。", "What's in the jar won't run off."),
        ("jar", "zimeng", false) => tr!("……留着。", "…Leave it."),
        ("jar", "mashu", false) => tr!("下一颗给你。我说的。", "The next one's yours. I said so."),
        ("jar", "paopao", false) => tr!("……哦。我刚才也想去看。", "…Oh. I was going to look too."),
        ("jar", "gugu", false) => tr!("我……我不偷吃的。", "I… I don't sneak any."),
        ("jar", "rongrong", false) => tr!("嗯。慢慢吃。", "Mm. Slowly does it."),

        ("tune", "zimeng", true) => tr!(
            "刚才那段，第三个音我按重了。",
            "That piece just now. I hit the third note too hard."
        ),
        ("tune", "mashu", true) => tr!("我的弓今天有点紧。", "My bow's a bit tight today."),
        ("tune", "paopao", true) => tr!(
            "……我刚才吹出一个很圆的泡泡。",
            "…I blew a very round bubble just now."
        ),
        ("tune", "gugu", true) => tr!(
            "菌盖上有一个斑点，敲起来跑调。",
            "One of the spots on my cap plays out of tune."
        ),
        ("tune", "rongrong", true) => tr!(
            "胸前有一根弦，松了一点。",
            "One of my strings has gone a little slack."
        ),
        ("tune", "zimeng", false) => tr!("我没听出来。", "I didn't hear it."),
        ("tune", "mashu", false) => tr!(
            "下次你弹，我在旁边跟。",
            "Next time you play, I'll follow along beside you."
        ),
        ("tune", "paopao", false) => tr!("……我听着挺好的。慢慢的。", "…Sounded fine to me. Slow."),
        ("tune", "gugu", false) => {
            tr!("我、我也有一个敲不准的。", "I-I've got one that's off too.")
        }
        ("tune", "rongrong", false) => tr!(
            "放一放，它自己会回来。",
            "Leave it a while. It comes back by itself."
        ),

        ("edge", "zimeng", true) => tr!(
            "上次挂在屏幕边上，看到了很远。",
            "Last time I hung off the edge of the screen, I could see a long way."
        ),
        ("edge", "mashu", true) => tr!(
            "下次被甩上去，我想挂在你旁边。",
            "Next time I'm thrown up there, I want to hang next to you."
        ),
        ("edge", "paopao", true) => tr!(
            "……屏幕边上，有点滑。",
            "…The edge of the screen is a bit slippery."
        ),
        ("edge", "gugu", true) => tr!(
            "我、我不想被甩到边上去了。",
            "I-I don't want to be thrown at the edge again."
        ),
        ("edge", "rongrong", true) => tr!(
            "挂在边上的时候，什么都慢了。",
            "Hanging off the edge, everything slows down."
        ),
        ("edge", "zimeng", false) => tr!("我在下面看着。", "I'll watch from down here."),
        ("edge", "mashu", false) => tr!("那我在下面接你。", "Then I'll catch you down here."),
        ("edge", "paopao", false) => tr!("……我还没上去过。", "…I've never been up there."),
        ("edge", "gugu", false) => tr!(
            "我……我在下面替你看着。",
            "I'll… I'll keep watch for you down here."
        ),
        ("edge", "rongrong", false) => tr!("掉下来也不疼。", "Falling down doesn't hurt."),
        _ => String::new(),
    }
}

/// Who is writing, for the letter prompt: its temper, and what it plays.
pub fn nature(who: &str, en: bool) -> &'static str {
    match (who, en) {
        ("mashu", false) => "你的性子：黏人，总想挨近一点；你会写自己当时待在哪儿、离他有多近；直白，但不撒娇、不卖萌。你拉大提琴，用尾巴当弓。",
        ("mashu", true) => "Your temper: clingy, always wanting to be a little closer; you write where you were at the time and how near to them; plain-spoken, never cute or coy. You play the cello, with your tail for a bow.",
        ("paopao", false) => "你的性子：慢半拍，什么都晚一点才发现，发现了就老实写下来；句子短，常停顿。你吹长笛，吹出来的是泡泡。",
        ("paopao", true) => "Your temper: half a beat slow; you notice things late and write them down honestly when you do; short sentences, with pauses. You play the flute, and what comes out is bubbles.",
        ("gugu", false) => "你的性子：胆小，总躲在菌盖下面看；写得小声，不敢说满，但看得很细。你敲木琴，敲的是自己菌盖上的斑点。",
        ("gugu", true) => "Your temper: shy, always watching from under your cap; you write in a small voice and never overstate, but you see the small things. You play the xylophone, on the spots of your own cap.",
        ("rongrong", false) => "你的性子：不急，什么事都愿意等；写得慢而稳，不催人，也不安慰人。你拨竖琴，弦是自己胸前的卷毛。",
        ("rongrong", true) => "Your temper: in no hurry, willing to wait for anything; you write slowly and steadily, never pressing and never consoling. You play the harp, and its strings are the curls on your own chest.",
        (_, false) => "你的性子：话少，只写自己看到的；心意藏在细节后面。你弹钢琴，用两只垂下来的耳朵。",
        (_, true) => "Your temper: few words, writing only what you saw; what you feel stays behind the details. You play the piano, with your two long ears.",
    }
}

/// "弹了一段琴" / "拉了一段琴": how the diary says a pet played, by its instrument.
pub fn played(instrument: &str) -> &'static str {
    use crate::i18n::t;
    match instrument {
        "cello" => t("拉了一段琴", "played a piece on the cello"),
        "flute" => t("吹了一段笛", "played a piece on the flute"),
        "xylophone" => t("敲了一段琴", "played a piece on the xylophone"),
        "harp" => t("拨了一段琴", "played a piece on the harp"),
        _ => t("弹了一段琴", "played a piece on the piano"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_moment_has_a_line_for_everyone() {
        for who in [
            "zimeng",
            "mashu",
            "paopao",
            "gugu",
            "rongrong",
            "someone-new",
        ] {
            for key in [
                "back_stuck",
                "back_done",
                "back_both",
                "done",
                "late",
                "stuck",
                "sit_dance",
                "sit_kegel",
                "wait",
                "help_stuck",
                "join",
                "listen",
                "dance_too",
                "no_fish",
                "mm",
                "ask_play",
                "ask_dance",
                "letter_ready",
                "earn_kegel",
                "earn_break",
                "earn_catch",
                "offer_wait",
                "offer_late",
                "offer_tidy",
                "back_wait",
                "offer_stow",
                "night_done",
                "night_left",
                "night_disk",
                "give",
                "give_stuck",
                "take",
                "bump",
                "bump_back",
            ] {
                let l = super::line(who, key, 7);
                assert!(!l.is_empty(), "{who} {key}");
                assert!(!l.contains("{n}"), "{l}");
            }
        }
        for who in ["zimeng", "mashu", "paopao", "gugu", "rongrong", "someone-new"] {
            for key in ["noted", "ate", "wait", "carry", "night_done", "night_left"] {
                let l = super::todo(who, key, "Claude", "看 PR");
                assert!(!l.is_empty() && !l.contains('{'), "{who} {key} {l}");
            }
        }
        // No two of the five say it the same way.
        let said: std::collections::BTreeSet<String> =
            ["zimeng", "mashu", "paopao", "gugu", "rongrong"]
                .iter()
                .map(|w| super::line(w, "done", 20))
                .collect();
        assert_eq!(said.len(), 5);
        // Small talk: every pet can open and answer every topic.
        for who in [
            "zimeng",
            "mashu",
            "paopao",
            "gugu",
            "rongrong",
            "someone-new",
        ] {
            for topic in super::TOPICS {
                assert!(
                    !super::chat(who, topic, true).is_empty()
                        && !super::chat(who, topic, false).is_empty(),
                    "{who} {topic}"
                );
            }
        }
    }
}
