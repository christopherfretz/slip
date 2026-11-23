// §10.2, a use through a using-declaration: the name in `using
// lib::old_name;` is not a reference in version 0 (§6.5 rule 4), so the
// run neither edits nor reports it; verification catches the miss.
namespace lib {
int old_name(int);
}
using lib::old_name;
int use() { return old_name(1); }
