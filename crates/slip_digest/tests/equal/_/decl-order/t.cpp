#include "side.h"
// spec/digest.md §1 rule 2: declaration order within a namespace.
namespace ns {
#ifdef B
void g(char);
void f(int);
#else
void f(int);
void g(char);
#endif
}
