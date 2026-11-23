// spec: §6.5 rule 3
// What is a reference in version 0 (spec/api.md §6.5 rule 3): a name
// written in one of the roles of the table, when it was resolved while the
// unit was parsed. This transform is shared by one directory per role; its
// subject writes names in that role. Every reference to each entity below
// is reported at its extent (§6.6 rule 2), with the name looked up, so
// the expectation lists every written name of the subject that denotes one
// of them.
#include <slip/meta.h>

using namespace slip;

consteval void each(std::string_view what, info e) {
  for (info r : references_to(e)) {
    range x = extent_of(r);
    if (x)
      report(x, severity::note, what);
    else
      report(severity::note, std::string(what) + ": a reference with no extent");
  }
}

consteval void transform() {
  for (std::string_view name : {"N", "N::M", "N::M::v", "N::f", "N::g", "N::v", "N::A", "N::S",
                                "N::S::m", "N::S::f", "N::B", "N::D", "N::D::m", "N::V"})
    for (info e : lookup(name)) each(name, e);
  for (info h : lookup("N::h"))
    for (info p : parameters_of(h)) each("the parameter " + identifier_of(p), p);
}
