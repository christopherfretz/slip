// WP-104 part 2 (verify.md §6.1): two specialisations whose arguments differ only in a namespace have two names
namespace legacy { struct Mine {}; template <class T> struct store { T t; }; }
namespace other { struct Mine {}; }
legacy::store<legacy::Mine> a;
legacy::store<other::Mine> b;
legacy::store<legacy::store<other::Mine>> c;
legacy::store<legacy::store<legacy::Mine>> d;
