#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: as spelling-site-prefix, through the arguments of a variable template specialisation. Until WP-163 indeterminate: since Kiln WP-2720 (`dc5a7989e`) the parameter keeps its declared array type (`TypeView::Decayed`), whose spelling rule 2 reads; `f(long)` added changes the found set.
template<int N> constexpr int vt = 1;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, void (*)(decltype(T(vt<sizeof(sizeof(f(T())))>))[2]));
