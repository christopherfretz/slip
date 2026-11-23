#include "side.h"
// digest-kinds §11.9 (WP-045 review, N3): Kiln copies the pattern
// definition's `#pragma clang section` attribute onto the instantiation
// when it instantiates the definition (kiln_sema/src/section.rs:204): only
// `definition` differs.
template <class T> struct Q { void g(); };
#ifdef B
#pragma clang section text=".x"
#endif
template <class T> void Q<T>::g() {}
#pragma clang section text=""
Q<int> q;
void h() { q.g(); }
