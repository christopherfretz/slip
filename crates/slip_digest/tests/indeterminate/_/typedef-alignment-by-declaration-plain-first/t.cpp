#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a plain declaration, then an aligned one: Kiln's `alignof(R)` is 8; clang 19 gives 8. Since Kiln WP-2830 (`8e23911be`) a declaration carries its own requests and those before it, so a later declaration that adds a request the first lacks still makes the alignment depend on the declaration a use names (WP-163); a later request not among the first's is taken to disagree (digest-kinds §4.2 rule 9).
typedef int R;
typedef int R __attribute__((aligned(8)));
unsigned long g() { return alignof(R); }
struct S { char c; R r; };
