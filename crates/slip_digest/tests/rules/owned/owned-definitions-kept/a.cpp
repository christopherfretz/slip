#include "dep.h"
namespace legacy { struct Mine {}; }
int legacy::f() { return 1; }
int legacy::Dep::s = 3;
struct legacy::Dep::Nested { int n; };
void legacy::Dep::m() {}
int acme::g() { return 2; }
