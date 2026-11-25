#include "side.h"
// WP-156 (ruling 4's class): two operands, side B replaces the second by
// its type. The operand unit is on both sides and differs; the trace
// shows which operand (spec/verify.md §6.1).
namespace lib { struct X {}; int make(X); }
lib::X x;
#ifdef B
void f(decltype(lib::make(x)), int);
#else
void f(decltype(lib::make(x)), decltype(lib::make(x)));
#endif
