#include "side.h"
// §4.4 "Spellings", rule 11: a pseudo-destructor in a signature whose qualifier is dependent itself (`T::template Bq<..>::~Bq`): Kiln builds the destroyed type from it and keeps neither the qualifier (`expr_nns`) nor a first type-name (`pseudo_dtor_first`), so what the qualifier held beyond the destroyed type is not known: the unit is indeterminate (`spelling/pseudo-destructor-qualifier`).
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T *p) -> decltype(p->T::template Bq<sizeof(sizeof(f(T())))>::~Bq(), 0) { return 0; }
