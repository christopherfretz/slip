#include "side.h"
// §4.4, F5 (WP-036), moved by WP-041 fix round 3: `decltype(p->T::~T())` in a signature. Kiln keeps that the name was qualified and not the qualifier, so side A is indeterminate (`spelling/pseudo-destructor-qualifier`, §4.4 "Spellings", rule 11); it was a pending `differ` test of the destroyed type in the profile.

#ifdef B
template<class T, class U> auto f(T* p) -> decltype(p->~T());
#else
template<class T, class U> auto f(T* p) -> decltype(p->T::~T());
#endif
