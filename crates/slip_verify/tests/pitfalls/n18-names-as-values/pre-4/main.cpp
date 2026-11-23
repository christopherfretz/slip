#include "dep.h"

namespace legacy {
int where() {
  char buf[sizeof(__PRETTY_FUNCTION__)];
  return sizeof buf;
}
}

int user() { return legacy::where(); }
