#include "dep.h"

namespace legacy {
class Mine {
  int secret = 1;
  friend int peek(const Probe &);
};
}

int user() { legacy::Mine m; return legacy::peek(legacy::Probe()); }
