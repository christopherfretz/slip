#include "side.h"
// digest-kinds §11.9 (WP-045 review, N7): the out-of-line pattern
// definition is inline on side B; Kiln marks the instantiation implicitly
// inline when it instantiates the definition (kiln_sema/src/class/members.rs:906).
template <class T> struct Q { void g(); };
#ifdef B
template <class T> inline void Q<T>::g() {}
#else
template <class T> void Q<T>::g() {}
#endif
Q<int> q;
void h() { q.g(); }
