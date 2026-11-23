#include "srcloc.h"
void log(std::source_location);
void f(int) {
  log(std::source_location::current());
}
// WP-104 (digest §9), review G1: the file of a passed-on std::source_location is part of its identity (by value, as a __FILE__ literal); its line is not
