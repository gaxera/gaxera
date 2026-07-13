# Session 05: The Deconstruction — Questioning Everything

> ⚠️ **HISTORICAL RECORD**
> This document is preserved from the project's design phase. It may receive editorial improvements for clarity, but its historical meaning and intent must remain unchanged.
> For current architecture, see [Technical Specification](../../spec/technical_spec.md).

Every concept taken for granted about computers was put on trial. Each concept must justify its existence from scratch or be sentenced to death.

---

## The Seven Sacred Concepts on Trial

### Trial 1: "Files"

#### History of Files

The concept of a "file" was invented in the 1950s-60s as a **metaphor for physical filing cabinets.** Before computers, offices stored information in paper files inside metal cabinets. Early computer scientists said: *"Let's make the computer work the same way."*

A file is a named sequence of bytes. That's all it is. It has no inherent meaning — the OS doesn't know if `report.docx` is a quarterly earnings summary or a love letter. It's just bytes with a name and a location.

#### The Problem Files Were Solving

In the 1960s, computers had very little storage. Programs were loaded from punch cards and magnetic tape. There needed to be a way to organize persistent data so programs could find it again. The file was the answer: give data a name, put it somewhere, find it later by name and location.

#### Does the File Problem Still Exist?

The "find data again" problem absolutely still exists. But the **solution** — named blobs of bytes organized in hierarchical folders — was designed for:

- Very small amounts of data (kilobytes, not terabytes)
- Single-user systems
- Sequential access (tape drives)
- A world where humans could remember where they put things

Today you have **millions** of files. You can't remember where anything is. You have duplicates everywhere. A photo exists in Downloads, in Photos, in a WhatsApp backup, in iCloud. Which one is the "real" one? The file abstraction has no answer.

#### Alternatives to Files That Were Tried

| System | What it did differently | What happened |
| --- | --- | --- |
| **IBM System/38 (1978)** | No files at all. Everything was an "object" in a single-level store. RAM and disk were unified. | Ahead of its time. Survived as IBM AS/400. Still runs banks today. Proved files aren't necessary. |
| **BeOS (1995)** | Files had rich metadata and attributes. You could query files like a database. | Beloved by creators. Killed by Microsoft's anticompetitive practices, not by bad design. |
| **WinFS (Microsoft, 2003)** | Attempted to replace NTFS with a relational database filesystem. | Cancelled after years of development. Too ambitious, too slow, too many compatibility issues. |
| **macOS Spotlight + Tags** | Bolted search and tagging onto the existing file system. | Half-measure. Better than nothing, but the file system underneath is still hierarchical. |

#### Provocative Questions on Files

> **If you had never seen a file system, and I told you: "You have a machine with a petabyte of storage. How should humans organize and retrieve information?" — would you invent files and folders?**
>
> **Or would you invent something closer to how your brain works — associations, memories, contexts, patterns?**
>
> **Is a "file" even the right unit of information? A file can be 1 byte or 100 gigabytes. A file can be a single sticky note or an entire movie. Should the same abstraction handle both?**

---

### Trial 2: "Applications"

#### History of Applications

The concept of an "application" (app) emerged in the 1970s-80s. Before apps, you had **tools** — small programs that did one thing well (the Unix philosophy). But as computers became graphical, software companies needed a way to sell big, feature-rich products. The "application" was born: a monolithic program that owns a specific domain.

Word processing? Buy Microsoft Word. Spreadsheets? Buy Excel. Photo editing? Buy Photoshop.

#### The Problem Applications Were Solving

Applications solve the problem of **packaging functionality.** A user needs to edit text, so they get an app that edits text. Simple.

#### Does the Application Problem Still Exist?

Yes — humans still need functionality. But the **app model** creates terrible problems:

- **Data imprisonment.** Your data lives inside the app. Your notes are in Notion. Your tasks are in Todoist. Your designs are in Figma. Want to connect a design to a task to a note to an email? Too bad. Each app is a walled kingdom.
- **Redundant reinvention.** Every app builds its own text editor, its own search, its own sharing mechanism, its own notification system. The same functionality is reimplemented a thousand times.
- **Cognitive overhead.** "Which app do I need for this?" is a question that shouldn't exist. The human has a GOAL. The goal doesn't naturally map to "an app."

#### Alternatives to Applications That Were Tried

| System | What it did differently | What happened |
| --- | --- | --- |
| **Unix Pipes (1970s)** | Small tools connected together. Output of one feeds into the next: `cat file \| grep "error" \| sort \| uniq -c` | Brilliant for text. Never scaled to graphical or structured data. |
| **OpenDoc (Apple, 1990s)** | Documents contained components from different apps. A single document could have a Word section, an Excel chart, and a Photoshop image — all live-editable. | Killed by Microsoft (who had competing OLE/COM tech) and internal Apple politics. |
| **Microsoft OLE/COM** | Similar to OpenDoc. Embed Excel charts in Word documents. | Technically works, practically horrible. Slow, buggy, confusing. |
| **Plan 9 (Bell Labs)** | Everything is a file, even network resources. No "apps" — just tools that operate on shared name spaces. | Too radical. Never achieved mainstream adoption. Influenced FUSE and 9P protocol. |

#### Provocative Questions on Applications

> **If apps didn't exist, and someone proposed: "Let's create giant monolithic programs that each imprison their own data and can't talk to each other" — would you accept that proposal?**
>
> **What if functionality was a SERVICE provided by the OS, not a product sold by a company? What if "text editing" was a system capability that ANY context could invoke?**
>
> **What if the unit of work wasn't an "app" but a "capability" — and the OS composed capabilities together based on what the user needs?**

---

### Trial 3: "The Desktop"

#### History of the Desktop

The desktop metaphor was invented at **Xerox PARC** in 1973 (the Alto), refined by Apple (Lisa, 1983; Macintosh, 1984), and copied by Microsoft (Windows, 1985). The idea: make the computer screen look like a physical office desk. Files are "documents." Folders are "directories." There's a "trash can." You "cut" and "paste" (from literally cutting paper with scissors).

#### The Problem the Desktop Was Solving

The desktop metaphor made computers approachable by mapping digital concepts to physical objects people already understood.

#### Does the Desktop Problem Still Exist?

The design team observed that this problem has largely vanished. With digital interfaces being universally understood by modern generations, the necessity of pretending the computer is a desk was heavily questioned.

But we're still pretending. In 2026, you still see:

- A flat 2D plane of icons
- "Folders" with little folder icons
- A "recycle bin" / "trash can"
- "Documents" you "open" and "close"

All of this is a skeuomorphic crutch from 42 years ago.

#### Provocative Questions on the Desktop

> **If you had never seen a desktop GUI, and I said: "Design how a human should interact with a computing environment" — would you create a flat rectangle with tiny pictures on it?**
>
> **Mobile OSes already abandoned the desktop (no "desktop" on your phone). VR headsets abandoned it (no "desktop" in a Quest). Why do we assume the desktop is inevitable on a laptop/PC?**
>
> **What if there IS no "desktop"? What if the computing environment shapes itself around what you're doing right now?**

---

### Trial 4: "Windows"

#### History of Windows

Overlapping rectangular windows were invented at Xerox PARC in 1974. The idea: each application gets its own rectangle on screen. You can move, resize, minimize, and overlap them. This mimicked having multiple pieces of paper on a physical desk.

#### The Problem Windows Were Solving

Multitasking. Before windows, you could only see one program at a time. Windows let you see multiple programs simultaneously.

#### Does the Windows Problem Still Exist?

Yes — humans do multiple things. But overlapping windows are a **terrible** solution:

- You spend enormous time **managing** windows. Resizing, repositioning, alt-tabbing, finding the buried window.
- Windows overlap and hide each other. The information you need is always behind the window you're looking at.
- Most people end up with ONE maximized window anyway, defeating the entire purpose.
- Tiling window managers (i3, Sway) prove that manual window management is wasted effort.

#### Provocative Questions on Windows

> **How much of your computer time is spent actually WORKING vs. managing the rectangles your work lives in?**
>
> **What if the OS showed you exactly the information you needed, arranged exactly as you needed it, without you ever dragging a rectangle?**
>
> **Are windows an interface paradigm — or a coping mechanism for an OS that doesn't understand what you're doing?**

---

### Trial 5: "Processes"

#### History of Processes

The "process" was invented in the late 1960s (Multics). A process is an **isolated running instance of a program.** Each process gets its own virtual memory space, its own thread of execution, and can't (in theory) interfere with other processes.

#### The Problem Processes Were Solving

Before processes, if one program crashed, it took down the whole machine. Processes provide **isolation** — a crashing program can be killed without affecting anything else.

#### Does the Process Problem Still Exist?

Yes — isolation is critical. But the process model has deep issues:

- **Heavyweight.** Creating a process is expensive (memory, kernel resources). That's why we invented threads, coroutines, goroutines, async/await — all hacks to avoid the cost of processes.
- **Poor communication.** Processes can't easily share data. IPC (Inter-Process Communication) is complex and slow compared to direct function calls. This is why monolithic apps exist — it's easier to keep everything in one process.
- **Process ≠ Intent.** When you're writing code, you might have VS Code, a terminal, a browser, and a file manager open. That's 4+ processes. But to YOU, it's **one activity: coding.** The process boundary doesn't match the human boundary.

#### Provocative Questions on Processes

> **Is "a running program" the right unit of isolation? Or should isolation happen at a different boundary — like per-capability, per-intent, or per-data-domain?**
>
> **What if instead of "processes," the OS had "activities" or "contexts" — units that match human concepts rather than machine concepts?**

---

### Trial 6: "The Filesystem Hierarchy" (Directories/Folders)

#### History of Filesystem Hierarchy

The hierarchical filesystem (directories containing files containing bytes) was invented in **Multics (1965)** and refined in **Unix (1969)**. It's a tree structure: one root, branching into directories, which branch into subdirectories, all the way down to files.

#### The Problem Filesystem Hierarchy Was Solving

With only kilobytes of storage and dozens of files, a flat list was becoming unmanageable. A tree structure let you group related files together.

#### Does the Filesystem Hierarchy Problem Still Exist?

The organization problem exists. But trees are the WRONG structure for human knowledge:

- **Forced single-parent.** A file can only exist in ONE folder. But knowledge isn't a tree — it's a **graph.** A photo from a vacation is simultaneously: a family photo, a vacation memory, a photo from 2024, a photo from Barcelona, and a reference for a design project. Where does it "live"?
- **Arbitrary taxonomy.** Do you organize by project? By date? By type? By client? There's no right answer, and whatever you choose, you'll constantly be looking for things in the wrong place.
- **Human memory mismatch.** You remember "that document I was working on last Tuesday" or "the spreadsheet Sarah sent me about the Q3 budget." You DON'T remember `C:\Users\Bee\Documents\Work\Q3\Budget\Final\v2_revised_FINAL.xlsx`.

#### Provocative Questions on Filesystem Hierarchy

> **If human memory is associative (connecting ideas by relationships and context), why do we force an inherently hierarchical structure onto it?**
>
> **What if data didn't "live" anywhere? What if it simply existed, and you could find it by any relationship, attribute, time, context, or query?**

---

### Trial 7: "Users and Permissions"

#### History of Users and Permissions

Multi-user computing was invented because **computers were expensive.** In the 1960s-70s, a single mainframe served dozens or hundreds of users via terminals. Each user needed to be isolated from the others — you shouldn't be able to read my files or crash my programs.

#### The Problem Users and Permissions Were Solving

Shared, expensive hardware. Multiple humans on one machine.

#### Does the Users and Permissions Problem Still Exist?

For most personal computing use cases, no. A personal laptop is typically used by one individual. The multi-user model often persists as a legacy abstraction — there's an "Administrator" account, a "Guest" account, file permissions... for a machine that only ONE human uses.

The real security boundary today isn't user-vs-user. It's:

- **App vs. App** (should Chrome see your banking data from a different app?)
- **Intent vs. Intent** (should your work context see your personal data?)
- **Online vs. Offline** (should a downloaded program access your network?)

#### Provocative Questions on Users and Permissions

> **If most computers have ONE user, should "users" even exist as a concept? Or should security be about capabilities and contexts instead of identity?**
>
> **What if instead of "this USER can access this FILE," the model was "this ACTIVITY has access to this DATA"?**

---

## Historical Attempts to Rethink Everything

You're not the first person to question these sacred concepts. Here are the giants who came before — and why their visions died:

| Project | Year | What they reimagined | Why it died | The lesson |
| --- | --- | --- | --- | --- |
| **Engelbart's NLS** | 1968 | Hypertext, collaborative editing, video conferencing — 50 years before they became mainstream | Too far ahead. No ecosystem. Hardware couldn't support it. | Being right too early is the same as being wrong. Timing matters. |
| **Xerox Alto/Star** | 1973-81 | The GUI, the mouse, WYSIWYG, Ethernet, laser printing | Xerox didn't understand what they had. Apple and Microsoft took the ideas. | Innovation without execution is just research. |
| **Lisp Machines** | 1970s-80s | The entire OS was one language. No boundary between the OS and programs. Everything was inspectable, modifiable, alive. | High cost, hardware specialization, and the rapid improvement of commodity x86 hardware. | Elegance must survive economic realities. |
| **Smalltalk / Squeak** | 1972-present | Everything is an object. No files — persistent objects in a living image. The entire environment is mutable at runtime. | Too foreign for broad adoption. Developers struggled to transfer skills from other systems. | Radical unfamiliarity can be a significant barrier. |
| **Canon Cat** | 1987 | No files, no desktop, no apps. The entire machine was one continuous document. You typed, searched, and computed — all in the same space. | Commercial failure due to marketing missteps, internal politics, and being too unfamiliar. | If you remove everything familiar, you need to provide something immediately intuitive in its place. |
| **NeXT** | 1988-93 | Object-oriented OS, elegant development environment, years ahead of its time. | High hardware cost and a specialized market. | Survived as macOS's foundation — demonstrating that visionary software can outlive hardware failures. |
| **BeOS** | 1990s | Multimedia-first, pervasive multithreading, database-like filesystem. Blazingly fast. | A combination of market dominance by incumbents (e.g. OEM agreements) and a lack of key early software. | The best technology doesn't always win alone. Ecosystem, distribution, and timing matter. |
| **Oberon** | 1988 | Minimal, elegant, everything built from components. The entire OS fit in a few hundred kilobytes. | Academic project. Never pursued commercial adoption. | Purity without pragmatism stays in academia. |

### The Pattern

Every radical reimagining of computing died for one or more of these reasons:

1. **Too early** — hardware couldn't support the vision
2. **Too expensive** — couldn't compete on price
3. **Too alien** — users couldn't transfer existing habits
4. **Too isolated** — no app ecosystem, no developer adoption
5. **Killed by monopolies** — incumbents used market power to suffocate them

> [!WARNING]
>
> ### The Lesson For Us
>
> We are attempting something that has been attempted before by some of the most brilliant engineers in history. They all failed — not because they were wrong, but because of **economic, social, and ecosystem forces.**
>
> Our philosophy must be revolutionary, but our **strategy** must account for these killers. We need:
>
> - A compatibility story (which we're planning — universal app support)
> - A migration path (users can't be thrown off a cliff)
> - The right timing (AI, Rust, modern hardware — the timing may finally be right)
> - Pragmatism alongside idealism

---

## Design Questions That Emerged

To push the project beyond superficial changes, the following questions were formulated to guide subsequent architectural thinking. They were carried into the next sessions without demanding immediate answers:

---

### On the Nature of Computing

**1.** When a human sits down at a computer, what is actually happening? Strip away all metaphors — no "desktop," no "opening apps," no "browsing the web." In the most fundamental terms: what is the human doing?

**2.** If computers had been invented by musicians instead of mathematicians, what would they look like? What about by architects? By chefs? How much of what we think is "fundamental" about computing is actually just the fingerprint of the kind of people who built the first ones?

**3.** Is "interaction" the right paradigm at all? We assume humans INTERACT with computers — give input, receive output. But is there a paradigm beyond interaction? What would a **symbiotic** paradigm look like, where the boundary between human thought and computer capability dissolves?

---

### On Information

**4.** What is the difference between data, information, knowledge, and wisdom? Should the OS handle all four? Should the OS even know the difference?

**5.** Right now, when you remember something, your brain doesn't "open a file." It reconstructs a memory from fragments, associations, and context — and it often changes slightly each time. Should a computer's information system work more like human memory? What would that even mean?

**6.** If nothing were ever "deleted" — if every version of every piece of information persisted forever — would that be liberating or suffocating? What changes?

---

### On Human Nature

**7.** Computers currently assume humans are logical, organized agents who make deliberate decisions about where to store data and how to structure work. But humans are messy, emotional, forgetful, creative, and chaotic. What happens when an OS is designed for ACTUAL humans instead of idealized ones?

**8.** What's the difference between attention and intention? When you sit at a computer, is the OS competing for your attention or serving your intention? Most current OSes have become attention-capture machines (notifications, feeds, badges). What does an intention-serving OS look like?

**9.** Does the computer have a responsibility to the human? If so, what is that responsibility? Is it to obey? To protect? To challenge? To amplify? To remember? To forget?

---

### On Architecture

**10.** Every OS in history has had a "fundamental unit" — the thing everything else is built from (bytes, files, objects, messages). If you had to pick one atom for our OS, what would it be? And should there even BE a single fundamental unit?

**11.** Should the OS know the difference between "important" and "trivial"? Should it be able to tell that a family photo is more precious than a downloaded meme? And if so — what gives it the right to make that judgment?

**12.** If our OS had existed for 1,000 years, what would it contain? What should survive across decades of use? What should decay naturally? Is digital permanence a feature or a curse?

---

## Where We Are Now

The seven sacred concepts were officially placed on trial, and the history of fallen visionary projects was mapped out. The design questions formulated here directly influenced the intensive methodology and architecture deep-dives that followed, serving as the philosophical backbone for Session 06 and beyond.
