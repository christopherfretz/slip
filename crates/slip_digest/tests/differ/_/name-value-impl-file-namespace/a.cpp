#include "srcloc.h"
consteval std::source_location la() {
  return std::source_location::current();
}
constexpr std::source_location g = la();
// WP-104 (digest §9), review G1: the file of a passed-on std::source_location is part of its identity (by value, as a __FILE__ literal); its line is not
