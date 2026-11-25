#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/noexcept`, an adjusted array parameter of a function type that is not a declaration's own holds the canonical element type, whose operand has no spelling of its own. Until WP-163 indeterminate: since Kiln WP-2720 (`dc5a7989e`) the parameter keeps its declared array type (`TypeView::Decayed`), whose spelling rule 2 reads; `f(long)` added changes the found set.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, void (*)(void (*[2])() noexcept(noexcept(f(T())))));
