#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: as spelling-site-prefix, through the arguments of a variable template specialisation.
template<int N> constexpr int vt = 1;
int f(int);
template<class T> void g(T, void (*)(decltype(T(vt<sizeof(sizeof(f(T())))>))[2]));
