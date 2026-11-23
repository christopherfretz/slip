// §10.2, unqualified and qualified uses: the name is replaced, the
// qualifier left as written.
namespace lib {
int old_name(int);
int in_lib() { return old_name(1); }
}
int out() { return lib::old_name(2) + ::lib::old_name(3); }
