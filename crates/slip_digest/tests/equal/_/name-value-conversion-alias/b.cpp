// WP-104 (digest §9), the __func__ of a conversion function names its canonical type
struct Widget {};
using Handle = Widget;
void take(const char *);
struct S { operator Handle() { take(__func__); return {}; } };
