#include "side.h"
// §4.4 "Spellings", rule 5: Kiln interns a conversion name by its canonical type, so a marked one has no spelling of its own: `spelling/conversion-name`.
int f(int);
#ifdef B
long f(long);
#endif
struct Y { operator int(); operator long(); };
template<class T, class U> auto g(T t, U u) -> decltype(t.operator decltype(f(U()))());
