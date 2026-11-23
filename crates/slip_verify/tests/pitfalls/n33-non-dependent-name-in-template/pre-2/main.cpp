#include "dep.h"
int helper() { return 3; }
namespace legacy {
template <class T> struct Mine : T {
  int get() { return helper(); }
};
}

struct B { int helper() { return 2; } };
int user() { return legacy::Mine<B>().get(); }
