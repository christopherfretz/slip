#include "dep.h"

namespace acme {
int where() {
  char buf[sizeof(__PRETTY_FUNCTION__)];
  return sizeof buf;
}
}

int user() { return acme::where(); }
