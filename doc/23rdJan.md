2) Clinic settings (single-tenant)

Already in code

GET /api/v1/clinic

PATCH /api/v1/clinic

Good additions

GET /api/v1/clinic/settings (full settings object if you add more fields later)

PATCH /api/v1/clinic/settings

GET /api/v1/clinic/meta (timezone, business hours, default slot minutes, etc.)

3) Users (dcms_user)

Already in code

GET /api/v1/users

POST /api/v1/users

GET /api/v1/users/{user_id}

PATCH /api/v1/users/{user_id}

POST /api/v1/users/{user_id}/disable

POST /api/v1/users/{user_id}/enable

Good additions

POST /api/v1/users/{user_id}/set_password (admin flow; alternative to reset-by-username)

GET /api/v1/users/{user_id}/sessions

POST /api/v1/users/{user_id}/sessions/revoke_all

GET /api/v1/users/lookup?username=...

GET /api/v1/roles (if you formalize role catalog later)

4) Patients (patient)

Already in code

POST /api/v1/patients

GET /api/v1/patients?query=...

GET /api/v1/patients/{patient_id}

PATCH /api/v1/patients/{patient_id}

GET /api/v1/patients/{patient_id}/summary

POST /api/v1/patients/{patient_id}/archive

POST /api/v1/patients/{patient_id}/restore

POST /api/v1/patients/{patient_id}/link_user/{user_id}

POST /api/v1/patients/{patient_id}/unlink_user

Good additions

GET /api/v1/patients/{patient_id}/timeline (appointments + treatments + invoices later)

GET /api/v1/patients/{patient_id}/appointments

GET /api/v1/patients/{patient_id}/treatments

GET /api/v1/patients/{patient_id}/balance (when invoicing exists)

POST /api/v1/patients/{patient_id}/merge (dedupe/merge patients)

GET /api/v1/patients/export.csv (admin-only)

5) Patient phone numbers (patient_phone / phone_number) + SMS (sms)

Already in code

GET /api/v1/patients/{patient_id}/phone_numbers

POST /api/v1/patients/{patient_id}/phone_numbers

POST /api/v1/phone_numbers/normalize

GET /api/v1/phone_numbers/{phone_number_id}

PATCH /api/v1/phone_numbers/{phone_number_id}

DELETE /api/v1/phone_numbers/{phone_number_id}

POST /api/v1/phone_numbers/{phone_number_id}/make_primary

GET /api/v1/phone_numbers/{phone_number_id}/sms

POST /api/v1/phone_numbers/{phone_number_id}/sms

GET /api/v1/sms (global search)

GET /api/v1/sms/{sms_id}

DELETE /api/v1/sms/{sms_id}

POST /api/v1/sms/bulk_send

POST /api/v1/sms/render

Good additions

GET /api/v1/patients/{patient_id}/sms (aggregate across phones)

POST /api/v1/sms/{sms_id}/mark_reviewed (if you add workflow fields)

POST /api/v1/sms/webhook/inbound (if integrating SMS provider)

POST /api/v1/sms/webhook/status (delivery receipts)

6) Calls (call)

Your DB has call records, but your current codebase (routes you shared) doesn’t expose call endpoints yet.

Suggested

GET /api/v1/calls (search/filter: by phone, date range, contact_type)

POST /api/v1/calls (log a call)

GET /api/v1/calls/{call_id}

PATCH /api/v1/calls/{call_id} (edit note, times)

DELETE /api/v1/calls/{call_id} (or soft delete)

GET /api/v1/patients/{patient_id}/calls (joined via phone numbers)

POST /api/v1/calls/webhook/inbound (if you integrate telephony)

7) Employees (employee) + employee phones (employee_phone)

Not implemented in your shared routes, but the DB design supports it.

Suggested employee CRUD

GET /api/v1/employees?query=...

POST /api/v1/employees

GET /api/v1/employees/{employee_id}

PATCH /api/v1/employees/{employee_id}

POST /api/v1/employees/{employee_id}/archive (or /disable)

POST /api/v1/employees/{employee_id}/restore

Employee ↔ user link (since employee.user_id is nullable)

POST /api/v1/employees/{employee_id}/link_user/{user_id}

POST /api/v1/employees/{employee_id}/unlink_user

Employee phones

GET /api/v1/employees/{employee_id}/phone_numbers

POST /api/v1/employees/{employee_id}/phone_numbers

GET /api/v1/employee_phone_numbers/{phone_number_id}

PATCH /api/v1/employee_phone_numbers/{phone_number_id}

DELETE /api/v1/employee_phone_numbers/{phone_number_id}

POST /api/v1/employee_phone_numbers/{phone_number_id}/make_primary

8) Positions + employee_position (multi-position staff)

Also not implemented in your shared routes, but strongly implied by the DB.

Positions

GET /api/v1/positions

POST /api/v1/positions (admin/manager)

GET /api/v1/positions/{position_id}

PATCH /api/v1/positions/{position_id}

POST /api/v1/positions/{position_id}/disable (soft)

POST /api/v1/positions/{position_id}/enable

Employee ↔ position assignment

GET /api/v1/employees/{employee_id}/positions

POST /api/v1/employees/{employee_id}/positions/{position_id} (assign)

DELETE /api/v1/employees/{employee_id}/positions/{position_id} (unassign)

POST /api/v1/employees/{employee_id}/positions/{position_id}/make_primary

9) Services (service_catalog)

Already in code

GET /api/v1/services (active services)

Good additions

POST /api/v1/services (admin/manager)

GET /api/v1/services/{service_id}

PATCH /api/v1/services/{service_id}

POST /api/v1/services/{service_id}/disable

POST /api/v1/services/{service_id}/enable

GET /api/v1/services/types (grouped by service_type)

PATCH /api/v1/services/reorder (update display_number batch)

10) Appointments (appointment)

Not implemented in routes you shared, but DB has a full appointment model.

Core

GET /api/v1/appointments?from=...&to=...&doctor_id=...&status=...

POST /api/v1/appointments

GET /api/v1/appointments/{appointment_id}

PATCH /api/v1/appointments/{appointment_id}

POST /api/v1/appointments/{appointment_id}/cancel

POST /api/v1/appointments/{appointment_id}/confirm

POST /api/v1/appointments/{appointment_id}/check_in (status “came”)

POST /api/v1/appointments/{appointment_id}/finish

Views

GET /api/v1/calendar/day?date=...

GET /api/v1/calendar/week?start=...

GET /api/v1/calendar/month?month=...

GET /api/v1/doctors/{employee_id}/calendar?from=...&to=...

Scheduling utilities

GET /api/v1/availability?doctor_id=...&date=...&duration_min=...

POST /api/v1/appointments/validate_no_overlap (dry-run)

11) Treatments (treatment, treatment_item, treatment_item_tooth)

Not implemented yet in your shared routes, but DB supports it well.

Treatment header

GET /api/v1/treatments?patient_id=...&from=...&to=...

POST /api/v1/treatments

GET /api/v1/treatments/{treatment_id}

PATCH /api/v1/treatments/{treatment_id}

DELETE /api/v1/treatments/{treatment_id} (or soft delete)

Treatment items (services performed in that visit)

GET /api/v1/treatments/{treatment_id}/items

POST /api/v1/treatments/{treatment_id}/items

PATCH /api/v1/treatment_items/{treatment_item_id}

DELETE /api/v1/treatment_items/{treatment_item_id}

Tooth mapping for items

GET /api/v1/treatment_items/{treatment_item_id}/teeth

POST /api/v1/treatment_items/{treatment_item_id}/teeth (bulk add tooth_ids)

DELETE /api/v1/treatment_items/{treatment_item_id}/teeth/{tooth_id}

PATCH /api/v1/treatment_item_teeth/{treatment_item_tooth_id} (edit note)

Convenience “patient chart” APIs (very useful for UI)

GET /api/v1/patients/{patient_id}/odontogram (latest state per tooth, derived)

GET /api/v1/patients/{patient_id}/odontogram/history?tooth_id=...

GET /api/v1/patients/{patient_id}/treatments/by_tooth?tooth_id=...

12) Tooth dictionary (tooth_definition)

Not implemented yet, but DB includes tooth_definition as a “dictionary”.

Suggested

GET /api/v1/teeth/definitions?system=FDI (or universal/palmer)

GET /api/v1/teeth/definitions/{tooth_id}

POST /api/v1/teeth/definitions (admin-only; probably seed-only)

PATCH /api/v1/teeth/definitions/{tooth_id}

13) “Home” + misc

Already

GET /home (role-based placeholder)

Good additions

GET /api/v1/health (db connectivity + version)

GET /api/v1/version (git hash/build time)

GET /api/v1/dashboard/metrics (counts: today appts, new patients, etc.)

GET /api/v1/audit/logs (if you add an audit table)