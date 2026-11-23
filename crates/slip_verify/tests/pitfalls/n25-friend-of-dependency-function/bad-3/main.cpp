#include "dep.h"

namespace acme {
class Mine {
  int secret = 1;
  friend int peek(const legacy::Probe &);
};
}

int user() { acme::Mine m; return legacy::peek(legacy::Probe()); }
