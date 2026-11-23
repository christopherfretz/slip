#include "dep.h"
void make(long) {}
namespace acme {
void make(int) {}
}

using namespace legacy;
using namespace acme;
void user() {
  Dep d;
  make(1);
}
