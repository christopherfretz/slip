#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): two declarations with different alignments: Kiln's `alignof(R)` reads the declaration a use names, its layout every one present when the class is first laid out.
typedef int R __attribute__((aligned(16)));
typedef int R __attribute__((aligned(8)));
unsigned long g() { return alignof(R); }
struct S { char c; R r; };
