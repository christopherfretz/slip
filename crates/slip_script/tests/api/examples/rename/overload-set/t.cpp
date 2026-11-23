// §10.2, an overload set: both functions are renamed, two rules.
namespace lib {
int old_name(int);
int old_name(double);
}
int use() { return lib::old_name(1) + lib::old_name(2.0); }
