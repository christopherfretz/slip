#include "side.h"
// digest-kinds §11.9 (WP-045 second fix round, mutant P4): Kiln copies
// the attributes of a member class's pattern definition onto the
// member of the instantiation when it instantiates its definition
// (`Substituter::class_attrs`, kiln_sema/src/templ/subst/anon.rs:140):
// both sides define X<int>::In, and only its `definition` differs.
template <class T> struct X { struct In; };
#ifdef B
template <class T> struct [[nodiscard]] X<T>::In { T v; };
#else
template <class T> struct X<T>::In { T v; };
#endif
X<int>::In i;
