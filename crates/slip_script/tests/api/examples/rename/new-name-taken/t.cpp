// §10.2, the new name is already declared: an error report at it,
// nothing renamed.
namespace lib {
int old_name(int);
int new_name(double);
}
int use() { return lib::old_name(1); }
