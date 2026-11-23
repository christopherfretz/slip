// std: c++20
#include "side.h"
// digest §4.3, §4.4 back-references, `backref-content-unit`: a content-keyed unit (a static_assert of a class, whose key is its digest, computed inside the class's stream), determinate, whose three slots share the clause `1 + 2`; the clause in redundant parentheses gives the same digests. `walk::backref_tests::the_content_unit_pair_shares_nodes` checks the back-references and that no unit is indeterminate.
struct A { long v[3]; };
#ifdef B
struct S { static_assert(A{{[0 ... 2] = (1 + 2)}}.v[2] == 3); };
#else
struct S { static_assert(A{{[0 ... 2] = 1 + 2}}.v[2] == 3); };
#endif
