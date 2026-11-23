// spec: §5.5
// As resolved (spec/api.md §5.5): a call through a using-declaration
// resolves to the function, not to the using-declaration. The referent of
// the callee's reference and the callee are `a::f`; the using-declaration
// `b::f` is a different entity (§6.1 rule 5).
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("a::f")[0];
  info u = lookup("b::f")[0];
  info c = calls_to(f)[0];
  report(extent_of(c), severity::note,
         callee_of(c) == f ? "callee: a::f" : (callee_of(c) == u ? "callee: b::f" : "callee: other"));
  info r = callee_reference_of(c);
  report(extent_of(r), severity::note,
         referent_of(r) == f ? "referent: a::f"
                             : (referent_of(r) == u ? "referent: b::f" : "referent: other"));
}
