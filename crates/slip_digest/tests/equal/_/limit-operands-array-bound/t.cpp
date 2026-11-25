#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B1): an
// array bound is a value only at the pin, so a name in it moves unseen
// (`lib::make` on side A, `::make<lib::X>` on side B, `int` both times):
// equal. Site 3 is reserved until Kiln keeps the bound as written (KR9
// §5.11).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
char buf[sizeof(make(x))];
struct F { char m[sizeof(make(x))]; };
void g(char p[sizeof(make(x))]);
