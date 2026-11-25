#include "side.h"
// WP-156 fix round 1 (W7; `spec/digest.md` §2.4 "What Kiln does not
// keep"): a declaration written outside its class, whose declarator's
// qualifier names the class by a template-id with an operand. Kiln keeps
// no qualifier as written for a declarator: a declaration outside a class
// that a template-id or `decltype` with an operand names anywhere in the
// translation unit is indeterminate in its operand unit
// (`kiln/declarator-qualifier`), through an alias template too.
namespace lib { int make(int); }
using lib::make;
template <int N> struct Q { static int s; };
template <int N> int Q<N>::s = 1;
template <> int Q<sizeof(make(0))>::s = 5;
struct Pl { static int s; };
template <int> using AL = Pl;
int AL<sizeof(make(0))>::s = 2;
