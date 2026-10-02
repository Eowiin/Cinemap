pub mod allocine;
pub mod departments;

use departments::get_departments;
use allocine::get_cinemas_from_department;

pub async fn import_cinemas() -> anyhow::Result<()> {
    let departments = get_departments();

    for department in departments {
        get_cinemas_from_department(department).await?;
    }
    Ok(())
}
