-- Acyclic match with many literal arms that must stay structured.

pick_label |n Int| -> String:
    if n is:
        case 0 => return "branch_a"
        case 1 => return "branch_b"
        case 2 => return "branch_c"
        case 3 => return "branch_d"
        case 4 => return "branch_e"
        else => return "branch_f"
    ;
;

result = pick_label(7)
io(result)
