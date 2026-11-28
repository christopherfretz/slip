// WP-156 fix round 3 (negative control; fix round 2's STOP (a), zz_r5,
// now a read): an explicit instantiation of a function with a non-type
// argument writes the entry of `at` (L4's T1), equal on both sides.
namespace legacy {
template <int N> int at() { return N; }
extern template int at<3>();
int u = at<3>();
}
