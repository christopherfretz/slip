#include "side.h"
// WP-156 item 3 (S5): an operand whose walk writes no reference adds
// nothing: `noexcept(true)` against no operand at all gives no operand
// unit on either side.
#ifdef B
void f() noexcept(true);
void (*fp)() noexcept(true) = f;
#else
void f() noexcept;
void (*fp)() noexcept = f;
#endif
