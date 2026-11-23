#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/profile`, a local class member's dependent `noexcept`, written as Kiln holds it.
int f(int);
template<class T> void g(T) { struct L { void k() noexcept(noexcept(f(T()))); }; }
