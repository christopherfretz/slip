// §10.2, the name written in a macro's replacement list: it cannot be
// edited, so an error report at the invocation (attached, §7.4 rule 3).
namespace lib {
int old_name(int);
}
#define CALL_OLD(x) lib::old_name(x)
int use() { return CALL_OLD(1); }
