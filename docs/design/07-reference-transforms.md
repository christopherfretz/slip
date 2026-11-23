# 07 — Reference transforms: the bar the general API has to clear

Status: **design input (2026-11-22)**, not a specification. The code in
this document is illustrative: the names of queries and edits follow
[01](01-transform-api.md) where it has them and are placeholders where
it does not. Nothing here is built beyond what `spec/api.md` says.

## 1. Why this document exists

The namespace migration ([03](03-equivalence.md), [01](01-transform-api.md)
§9.2) is the first use that proves Slip useful. It is not the goal. The
operator's statement of the goal (2026-11-22): general-purpose
programming of refactors that nobody has imagined yet; the examples
below came out of that conversation, and the API "has to be at least
this powerful".

So the M4 design of the transform API is judged against **both** the
namespace migration and the transforms of this document:

- each is written in full against the proposed API, with **no string
  operation on code** ([01](01-transform-api.md) §6 "The rule");
- each runs over many translation units with nothing shared between
  the evaluations of two translation units;
- each has a stated answer to "what does `slip verify` prove about the
  result", or a stated limit.

A transform is added here when a conversation or a real codebase
produces one the current list does not cover. The list only grows.

## 2. Remove a feature flag

A library has a switch class: an object constructed with a name, asked
at run time whether the feature is on.

```cpp
class Switch {
public:
  explicit Switch(const char *name);
  bool isOn() const;
};
```

The feature `"fast_path"` has shipped. The transform's parameters are
the switch class, the name, and the value the flag now always has
(`on`). It must replace every test by the branch that survives and
remove the switch when, and only when, every use was handled.

### 2.1 Form 1: a switch object tested where it is declared

```cpp
// before
static Switch fast_path("fast_path");

int render(Scene &s) {
  if (fast_path.isOn()) {
    Renderer r(s);
    return r.run();
  } else {
    return legacy_render(s);
  }
}
```

```cpp
// after
int render(Scene &s) {
  Renderer r(s);
  return r.run();
}
```

What the transform has to be able to say:

- **Find the flag**: constructions of `Switch` whose argument is a
  constant equal to `"fast_path"` (a construction is a call;
  `constant_value_of(argument)`), and the variable each one initialises.
- **Classify every reference** to that variable: the object of an
  `isOn()` call or something else. Something else (the address taken, a
  reference bound, passed to a function) is not handled: a report at the
  site, and the declaration stays.
- **See statement structure**: the parent of a node, `is_if`, the
  condition, the branches; the same for `?:`, `while`, `!x`, `x && y`,
  `x || y` when the test is an operand.
- **Edit structurally**: "replace this `if` by its `then` branch,
  unwrapped", "remove this declaration". The unwrap primitive carries
  its checks: every name inside the block and after it in the enclosing
  scope resolves as before (the operator asked for shadowing on unwrap
  to be **detected and diagnosed**, not silently fixed), and no object
  with a non-trivial destructor changes lifetime unless the block was
  the last statement of its scope. A failed check is a diagnostic at the
  site and no edit.
- **Declare the change** more finely than "this function changed": the
  kept statements mean what they meant (§5).

### 2.2 Form 2: the switch is a data member, used across translation units

```cpp
// widget.h
class Widget {
  Switch fast_path_;
  int size_;
public:
  Widget();
  void draw();
  void resize();
};

// widget.cpp
Widget::Widget() : fast_path_("fast_path"), size_(0) {}
void Widget::draw() { if (fast_path_.isOn()) drawFast(); else drawSlow(); }

// resize.cpp
void Widget::resize() { if (!fast_path_.isOn()) relayout(); }
```

`resize.cpp` sees the member and a test on it. It does not see the
constructor, so it cannot know that the member is the flag
`"fast_path"`. `widget.cpp` knows, and cannot see `resize.cpp`. No
translation unit can decide alone, and the evaluations share nothing.

What this adds:

- **Facts keyed by an entity** (§4): "this member is initialised with
  the flag's name by every constructor", "some use of this member could
  not be handled".
- **Guarded edits**: `resize.cpp` emits its edit *as if* the member were
  the flag, under a guard that only the fold over all translation units
  can decide.
- **A group that spans translation units**: all edits for one member
  apply or none do, although they come from different evaluations. The
  group's identity is derived from the entity, not created per
  evaluation.
- **List-aware removal**: a mem-initialiser with its comma or the colon
  when it was the only one; the member's declaration. The same primitive
  serves arguments, parameters, bases, enumerators, captures and
  template arguments.
- **A declared removal** (`expect_removed(member)`): the verifier is
  told the entity goes away, and checks that nothing else did.
- **A translation unit that fails to parse is an objection**: it may
  hold a use nobody saw.

### 2.3 Form 3: a `bool` copied from the flag

The operator's "uglier" case: the flag's value is read once into a
plain `bool`, and the code branches on the `bool`.

```cpp
class Engine {
  bool fast_;
  int n_;
public:
  Engine() : fast_(Switch("fast_path").isOn()), n_(0) {}
  explicit Engine(int n) : fast_(g_fast_path.isOn()), n_(n) {}
  int step() { return fast_ ? stepFast() : stepSlow(); }
  bool ready() const { return fast_ && n_ > 0; }
};
```

```cpp
// after
class Engine {
  int n_;
public:
  Engine() : n_(0) {}
  explicit Engine(int n) : n_(n) {}
  int step() { return stepFast(); }
  bool ready() const { return n_ > 0; }
};
```

What this adds:

- **A fold under an assumption**:
  `constant_value_of(expr, assuming(call_to(Switch::isOn, flag), true))`
  gives `true` for both initialisers. The transform does not pattern
  match the initialiser's shape; the evaluator folds it.
- **How a reference uses its target**: `access_of(ref)` is a read (the
  lvalue-to-rvalue conversion's operand), a write, or an escape (address
  taken, bound to a reference, captured by reference). The member is a
  constant only if every constructor initialises it to the same value
  under the assumption (a default member initialiser counts; the
  implicit copy and move constructors copy a value that is already
  that constant) and no translation unit writes it or lets it escape.
  That is a join of facts across translation units, again.
- **A residual expression**: `fast_ && n_ > 0` under `fast_ == true` is
  `n_ > 0`. The transform asks for the expression that remains; it does
  not assemble one. An operand that is dropped must have no side effect,
  or the edit is refused at the site.
- **A stated limit**: for a public member the answer holds inside the
  compilation database and nowhere else. What reads the object's bytes
  (`memcpy`, serialisation, `offsetof`, a layout another binary relies
  on) is not a reference; removing the member changes the class's
  layout, and the verifier will show every unit that depends on it. The
  design has to say how such a change is declared and reviewed, not
  hide it.

## 3. What the examples demand of the API

| Need | First needed by | Where it stands |
|---|---|---|
| Constructions as calls; the variable a construction initialises | 2.1 | version 0 has calls; constructions to check |
| `constant_value_of(expr)` | 2.1 | not built |
| Statement structure: parent, `is_if`, condition, branches | 2.1 | the least specified part of [01](01-transform-api.md) |
| Structural edits: remove a statement, unwrap a block (with its checks), replace a node by a copy of a node | 2.1 | rule stated in [01](01-transform-api.md) §6; primitives not designed |
| Shadowing on unwrap detected and diagnosed | 2.1 | a check of the unwrap primitive |
| Facts keyed by an entity, folded across translation units | 2.2 | designed ([01](01-transform-api.md) §7), not built |
| Guarded edits; a group spanning translation units | 2.2 | designed, not built; the spanning group is new |
| List-aware removal (the element with its separator) | 2.2 | not designed |
| `expect_removed` | 2.2 | not designed |
| A fold under an assumption | 2.3 | not designed; a request to Kiln's evaluator |
| `access_of(ref)`: read, write, escape | 2.3 | Kiln's written references give the node; the classification is new |
| A residual expression under an assumption, with a side-effect check | 2.3 | not designed |
| A declared change finer than a whole function; verification under an assumption | all | open (§5) |

## 4. How a decision crosses translation units

This is the answer given to the operator's question "how does the
context get carried across?". There is no `get_fact` in a transform,
and nothing traces one evaluation from another.

1. **Map** (one evaluation per translation unit, in parallel, sharing
   nothing). An evaluation does two write-only things. It
   `contribute`s to facts, each keyed by the **entity key** of the thing
   it is about (the key is the same in every translation unit that sees
   the entity: that is what makes the join possible). And it emits its
   edits, **always**, each carrying a guard: an opaque value built from
   `holds(fact)` that the evaluation itself cannot inspect.
2. **Reduce** (once, after every evaluation). Each fact is folded by the
   monoid its declaration names (`all_of`, `any_of`, a sum, a set
   union), so the result does not depend on the order or the number of
   contributions. Each guard is now a boolean; an edit group whose guard
   is false is dropped whole.

For form 2, in every translation unit:

```cpp
// Illustrative. `m` is a data member of type Switch.
constexpr fact<bool, all_of> is_flag;   // every constructor seen names the flag
constexpr fact<bool, any_of> blocked;   // some use could not be handled

for (info ctor : constructor_definitions_seen_for(class_of(m)))
  contribute(is_flag[m], initialises_with_name(ctor, m, "fast_path"));

for (info ref : references_to(m)) {
  if (info test = as_is_on_test(ref))
    replace_by_branch(test, on, {.atomic = group_of(m),
                                 .when = holds(is_flag[m]) && !holds(blocked[m])});
  else
    contribute(blocked[m], true);
}
```

`resize.cpp` contributes nothing to `is_flag[m]` (it saw no
constructor) and emits its edit anyway. `widget.cpp` contributes `true`
and emits the removal of the mem-initialiser and its own edit. The
header's removal of the member is emitted by both and is one edit after
de-duplication ([02](02-pipeline.md)). Reduce folds; the group applies
or it does not. A class with no constructor definition anywhere folds
`all_of` over nothing: the design must make that case an explicit
`false` (a fact may require at least one contribution).

**The limit: guards decide, they do not discover.** A guard chooses
between edits that were already written down. When *which* edit to
write depends on something only another translation unit knows (the
flag's value, were it not a parameter), the transform has three ways
out, in order of cost: emit each alternative under complementary
guards; a `reduce` entry point evaluated once with the folded facts
bound; a second round, whose map phase reads the first round's folded
facts as input. The second round is a full parse again and is the last
resort.

## 5. What verification means for these transforms (open)

The namespace migration's claim is "every entity is the entity it was".
A flag removal changes behaviour on purpose; its claim is "the program
is the one you get by assuming the flag is on". Two candidate answers
for the M4 design, not exclusive:

- **Declared changes with a shape.** Today a transform can declare
  `changed f` and the verifier then accepts any difference in `f`. The
  examples need a finer statement: "`f` differs by the replacement of
  this `if` by its `then` branch", checked by digesting the kept
  statements on both sides.
- **Equality under a declared assumption.** The run declares the
  assumption (`Switch("fast_path").isOn()` is `true`). The verifier
  normalises the **pre-image** under it (folds the tests, drops dead
  branches, unwraps) and requires the result to equal the post-image.
  This is the same comparison Slip already has, with a normalising
  walk on one side; it would prove the transform rather than bound it.
  Its cost and what the normalisation must cover are unknown.

Either way a removed data member changes layout, and every unit that
depends on the layout differs. The design must say how that set is
declared in one line and reviewed, not unit by unit.

## 6. Further transforms to test the design against

Candidates of the supervisor's, not requests of the operator; each is
here because it stresses something the two reference transforms do not.

- **Change a signature**: add, remove or reorder a parameter at every
  declaration, definition, call, pointer-to-function use and override
  (list-aware edits on both sides; virtual overrides as a join across
  translation units; macros that forward arguments).
- **Replace an out-parameter by a return value** (a structural rewrite
  of callers that changes statement shape).
- **Migrate an API with a changed ownership rule** (`T *` to a smart
  pointer): needs the read / write / escape classification of 2.3 over
  whole functions.
- **Split a header**, moving declarations and fixing includes: file
  creation, include management, and the verifier's "nothing resolved
  differently" at full strength.
- **Generate code from the model** (a serialiser per reflected class):
  whole-cloth generation ([01](01-transform-api.md) §10) with parsed
  fragments and typed holes only.
