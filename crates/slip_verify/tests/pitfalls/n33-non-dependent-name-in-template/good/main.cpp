#include "dep.h"

namespace acme {
template <class T> struct Mine : T {
  int get() { return legacy::helper(); }
};
}

struct B { int helper() { return 2; } };
int user() { return acme::Mine<B>().get(); }
