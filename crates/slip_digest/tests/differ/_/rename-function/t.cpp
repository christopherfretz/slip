#include "side.h"
namespace ns {
#ifdef B
void g(int);
#else
void f(int);
#endif
struct S { int x; void m() const; };
}
