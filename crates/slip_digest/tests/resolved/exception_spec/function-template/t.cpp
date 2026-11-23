#include "side.h"
// digest-kinds §11.2 rule 3, a function template's specialisation:
// declared on both sides by the explicit instantiation declaration, its
// specification resolved by side B's use only.
template <class T> void g(T) noexcept(sizeof(T) > 1);
extern template void g<int>(int);
#ifdef B
bool h() { return noexcept(g(1)); }
#endif
