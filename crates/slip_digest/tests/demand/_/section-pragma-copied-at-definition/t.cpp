#include "side.h"
// digest-kinds §11.9 (WP-045 review, N3): the section attribute Kiln
// copies from the pattern's definition when the use instantiates it
// (kiln_sema/src/section.rs:204) is `definition`'s.
template <class T> struct Q { void g(); };
#pragma clang section text=".x"
template <class T> void Q<T>::g() {}
#pragma clang section text=""
Q<int> *p;
#ifdef B
void use(Q<int> &q) { q.g(); }
#endif
