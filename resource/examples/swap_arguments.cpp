// swap_arguments.cpp -- replace lib::foo(a, b) with lib::bar(b, a).
//
// A complete Slip transform against <slip/meta.h> version 0
// (spec/api.md §10.1).
//
// For each pair in the table below, every call of `from` that is written
// with two arguments is rewritten to call `to` with the arguments exchanged.
// Give the table on the transform's command line:
//
//     -DSWAP_PAIRS='{"lib::foo","lib::bar"},{"lib::lt","lib::gt"}'
//
// The three edits of one call (the callee's name and the two arguments) are
// one atomic group. The arguments are copied, not retyped, so their
// formatting, their comments and any macro they use are kept.
//
// What it does not rewrite, it reports: a call without exactly two written
// arguments, a call that is not plain text Slip may edit (one that comes out
// of a macro's replacement list, one in a file that is not owned), and every
// place where the function is named and not called.

#include <slip/meta.h>

#if !defined(__slip_lib_meta_calls) || !defined(__slip_lib_meta_code)
#error "this transform needs the call queries and code values of <slip/meta.h>"
#endif

using namespace slip;

namespace {

struct swap_pair {
  std::string_view from;
  std::string_view to;
};

#ifndef SWAP_PAIRS
#  define SWAP_PAIRS {"lib::foo", "lib::bar"}
#endif

constexpr swap_pair pairs[] = {SWAP_PAIRS};

consteval std::string join(std::string_view a, std::string_view b, std::string_view c) {
  std::string s(a);
  s += b;
  s += c;
  return s;
}

consteval bool contains(const std::vector<info>& list, info x) {
  for (info i : list)
    if (i == x) return true;
  return false;
}

// The function with two parameters that has this qualified name in the
// subject unit. Null if there is none. If there are several, an error
// report and null: the pair is ambiguous and nothing is rewritten.
consteval info binary_function(std::string_view name) {
  info found;
  for (info e : lookup(name)) {
    if (!is_function(e) || parameters_of(e).size() != 2) continue;
    if (found) {
      report(severity::error,
             join("more than one function with two parameters is named '", name, "'"));
      return info();
    }
    found = e;
  }
  return found;
}

consteval void swap_calls(info from, info to, std::string_view why) {
  // The references that are the callee of a call seen below.
  std::vector<info> callees;

  for (info call : calls_to(from)) {
    info callee = callee_reference_of(call);
    callees.push_back(callee);

    std::vector<info> args = arguments_as_written(call);
    if (args.size() != 2) {
      report(call, severity::review,
             "not rewritten: the call does not have two written arguments");
      continue;
    }

    // The whole call must be one piece of editable text. If it is not (the
    // parentheses come from a macro's replacement list, say), then text
    // that belongs to something else could be caught by the exchange.
    range name = extent_of(callee);
    range first = extent_of(args[0]);
    range second = extent_of(args[1]);
    if (!is_editable(extent_of(call)) || !is_editable(name) || !is_editable(first) ||
        !is_editable(second)) {
      report(call, severity::review,
             "not rewritten: the call is not plain text that Slip may edit");
      continue;
    }

    group g = new_group();
    replace(name, spell(to), {.atomic = g});
    replace(first, copy(second), {.atomic = g});
    replace(second, copy(first), {.atomic = g});
    expect_changed(call, why);
  }

  for (info use : references_to(from))
    if (!contains(callees, use))
      report(use, severity::review,
             "not rewritten: the function is named here and not called");
}

}  // namespace

consteval void transform() {
  for (const swap_pair& p : pairs) {
    info from = binary_function(p.from);
    if (!from) continue;  // not declared in this unit, so nothing here calls it

    info to = binary_function(p.to);
    if (!to) {
      for (info call : calls_to(from))
        report(call, severity::review,
               "not rewritten: the replacement function is not declared in this "
               "translation unit");
      continue;
    }

    std::string why = join(p.from, "(a, b) -> ", p.to);
    why += "(b, a)";
    swap_calls(from, to, why);
  }
}
