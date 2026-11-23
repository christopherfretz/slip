#include "side.h"
// digest-kinds §11.9 (WP-045 review, N2/N19): Kiln copies `weak` from the
// out-of-line pattern definition onto the instantiation when it
// instantiates the definition (kiln_sema/src/templ/class.rs:1603-1615):
// only `definition` differs.
template <class T> struct Q { void g(); };
#ifdef B
template <class T> __attribute__((weak)) void Q<T>::g() {}
#else
template <class T> void Q<T>::g() {}
#endif
Q<int> q;
void h() { q.g(); }
