// §10.2, a class and a function named `lib::old_name`: the name denotes
// something that is not a function, an error report, nothing renamed.
namespace lib {
struct old_name {};
int old_name(int);
}
int use() { return lib::old_name(1); }
