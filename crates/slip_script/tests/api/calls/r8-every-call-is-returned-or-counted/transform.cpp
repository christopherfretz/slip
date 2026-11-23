// spec: §6.7 rule 8
// counters: api.calls_without_reference
// xfail: the host does not put api.calls_without_reference in the run output (spec/landed/api/WP-086.md decision 7 makes it a method of the subject); spec/api.md §6.7 rule 8 names the counter, and §7.6 rule 6 does not list it
// Every expression of rule 1's shape in the subject unit is a call or is
// counted (spec/api.md §6.7 rule 8). The unit's expressions include those
// of `noexcept`, `decltype`, template arguments and `alignas`, so their
// calls are returned. A member named after `template` (`d.template
// t<int>()`) is a call. A destructor called by name (`p->~D()`) is counted
// in `api.calls_without_reference`, once.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"f", "c", "D::t"})
    for (info call : calls_to(lookup(name)[0]))
      report(extent_of(call), severity::note, "a call of " + std::string(name));
}
