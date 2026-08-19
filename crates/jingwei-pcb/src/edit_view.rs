use crate::{Board, EditControl, Footprint, PcbError, Side, TextItem, Track, Via, Zone};
use jingwei_geometry::{
    Angle, BoundingBox, Length, Point, Polygon, Polyline, QuantizationPolicy, SpatialEntry,
    SpatialIndex,
};
use serde::{Deserialize, Serialize};

/// PCB viewport query.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewRequest {
    /// Visible board-space bounds.
    pub viewport: BoundingBox,
    /// Visible layer IDs; empty means all layers.
    pub visible_layers: Vec<String>,
    /// Level of detail, where zero is the coarsest.
    pub lod: u8,
}

/// Display-list geometry independent from the UI renderer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DisplayGeometry {
    /// Axis-aligned bounds.
    Bounds {
        /// Display bounds.
        bounds: BoundingBox,
    },
    /// Polyline centerline.
    Polyline {
        /// Centerline path.
        path: Polyline,
        /// Stroke width.
        width: Length,
    },
    /// Polygon boundary.
    Polygon {
        /// Polygon boundary.
        boundary: Polygon,
    },
    /// Circle.
    Circle {
        /// Circle center.
        center: Point,
        /// Circle diameter.
        diameter: Length,
    },
    /// Text anchor and content.
    Text {
        /// Text anchor.
        position: Point,
        /// Display text.
        text: String,
    },
}

/// Stable-ID display item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayItem {
    /// JIR candidate entity ID.
    pub entity_id: String,
    /// Entity kind.
    pub entity_kind: String,
    /// Optional layer.
    pub layer_id: Option<String>,
    /// Optional assigned net.
    pub net_id: Option<String>,
    /// Renderer-independent geometry.
    pub geometry: DisplayGeometry,
}

/// Deterministic viewport response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayList {
    /// Source candidate revision.
    pub candidate_revision: String,
    /// Sorted display items.
    pub items: Vec<DisplayItem>,
}

/// Hit-test response with stable entity IDs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitTestResult {
    /// Snapped query coordinate.
    pub snapped_point: Point,
    /// Candidate IDs from smallest/most specific to largest.
    pub entity_ids: Vec<String>,
}

/// Cross-probe response across logical, PCB, BOM and finding identity spaces.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossProbeResult {
    /// Logical component or pin IDs.
    pub logical_refs: Vec<String>,
    /// PCB entity IDs.
    pub pcb_refs: Vec<String>,
    /// BOM reference designators.
    pub bom_designators: Vec<String>,
    /// Finding IDs supplied by a caller-side index.
    pub finding_refs: Vec<String>,
}

/// Candidate-only PCB semantic command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SemanticEditCommand {
    /// Move a footprint.
    MoveFootprint {
        /// Footprint ID.
        entity_id: String,
        /// Requested center before normalization.
        position: Point,
    },
    /// Rotate a footprint.
    RotateFootprint {
        /// Footprint ID.
        entity_id: String,
        /// Requested rotation.
        rotation: Angle,
    },
    /// Flip a footprint between assembly sides.
    FlipFootprint {
        /// Footprint ID.
        entity_id: String,
    },
    /// Replace the board outline candidate.
    EditBoardOutline {
        /// New validated outline.
        outline: Polygon,
    },
    /// Add a route segment.
    AddTrack {
        /// New route candidate.
        track: Track,
    },
    /// Add a via.
    AddVia {
        /// New via candidate.
        via: Via,
    },
    /// Move an existing via.
    MoveVia {
        /// Via ID.
        entity_id: String,
        /// Requested center before normalization.
        position: Point,
    },
    /// Delete a track, via, zone or text entity.
    DeleteEntity {
        /// Entity ID.
        entity_id: String,
    },
    /// Add or replace a copper zone candidate.
    UpsertZone {
        /// Zone value.
        zone: Zone,
    },
    /// Add or replace text.
    UpsertText {
        /// Text value.
        text: TextItem,
    },
    /// Change edit control for a footprint, track or via.
    SetEditControl {
        /// Entity IDs.
        entity_ids: Vec<String>,
        /// New edit control.
        control: EditControl,
    },
    /// Change the default track width for a named class.
    SetNetClassWidth {
        /// Net-class ID.
        class_id: String,
        /// New width.
        width: Length,
    },
}

/// Non-mutating semantic edit preview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPreview {
    /// Normalized commands after coordinate snapping.
    pub normalized_commands: Vec<SemanticEditCommand>,
    /// Affected stable IDs.
    pub affected_entity_ids: Vec<String>,
    /// Affected net IDs.
    pub affected_net_ids: Vec<String>,
    /// Cheap local findings.
    pub local_findings: Vec<String>,
    /// Whether the full command group can be committed as a candidate.
    pub committable: bool,
}

/// Isolated candidate returned by a semantic edit commit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditCandidate {
    /// Base candidate revision.
    pub base_candidate_revision: String,
    /// Result board value; the kernel still owns transaction commit.
    pub board: Board,
    /// Stable affected objects.
    pub affected_entity_ids: Vec<String>,
    /// Validation scopes invalidated by this edit.
    pub invalidated_scopes: Vec<String>,
}

impl Board {
    /// Returns a renderer-independent display list with viewport culling.
    pub fn view(&self, request: &ViewRequest) -> DisplayList {
        let mut items = self.display_items();
        items.retain(|item| {
            let visible = item.layer_id.as_ref().is_none_or(|layer| {
                request.visible_layers.is_empty() || request.visible_layers.contains(layer)
            });
            visible && item_bounds(item).intersects(request.viewport)
        });
        items.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        DisplayList {
            candidate_revision: self.candidate_revision.clone(),
            items,
        }
    }

    /// Hit-tests board entities using deterministic snapping and stable IDs.
    pub fn hit_test(&self, point: Point, policy: &QuantizationPolicy) -> HitTestResult {
        let snapped = policy.snap(point);
        let entries: Vec<_> = self
            .display_items()
            .into_iter()
            .map(|item| SpatialEntry {
                bounds: item_bounds(&item),
                entity_id: item.entity_id,
            })
            .collect();
        let index = SpatialIndex::new(entries);
        let mut ids: Vec<_> = index
            .hit_test(snapped)
            .into_iter()
            .map(|entry| entry.entity_id.clone())
            .collect();
        ids.sort_by_key(|id| {
            let priority = if id.starts_with("pcb-pad") || id.starts_with("track") {
                0
            } else if id.starts_with("via") {
                1
            } else {
                2
            };
            (priority, id.clone())
        });
        HitTestResult {
            snapped_point: snapped,
            entity_ids: ids,
        }
    }

    /// Resolves a stable ID across logical, PCB and BOM identity spaces.
    pub fn cross_probe(&self, entity_id: &str, finding_refs: Vec<String>) -> CrossProbeResult {
        let mut result = CrossProbeResult {
            finding_refs,
            ..CrossProbeResult::default()
        };
        if let Some(footprint) = self.footprints.iter().find(|item| {
            item.id == entity_id
                || item.logical_component_id == entity_id
                || item.designator == entity_id
        }) {
            result
                .logical_refs
                .push(footprint.logical_component_id.clone());
            result.pcb_refs.push(footprint.id.clone());
            result.pcb_refs.extend(
                self.pads
                    .iter()
                    .filter(|pad| pad.footprint_id == footprint.id)
                    .map(|pad| pad.id.clone()),
            );
            result.bom_designators.push(footprint.designator.clone());
        } else if let Some((pin, pad)) = self
            .pin_pad_map
            .iter()
            .find(|(pin, pad)| pin.as_str() == entity_id || pad.as_str() == entity_id)
        {
            result.logical_refs.push(pin.clone());
            result.pcb_refs.push(pad.clone());
        } else if self.tracks.iter().any(|track| track.id == entity_id)
            || self.vias.iter().any(|via| via.id == entity_id)
        {
            result.pcb_refs.push(entity_id.to_owned());
        }
        result
    }

    /// Previews a command group without mutating the board.
    pub fn preview_edits(
        &self,
        commands: &[SemanticEditCommand],
        policy: &QuantizationPolicy,
    ) -> EditPreview {
        let mut board = self.clone();
        let mut normalized = Vec::new();
        let mut affected = Vec::new();
        let mut nets = Vec::new();
        let mut findings = Vec::new();
        for command in commands {
            let command = normalize_command(command.clone(), policy);
            match board.apply_edit(&command) {
                Ok((entity_ids, net_ids)) => {
                    affected.extend(entity_ids);
                    nets.extend(net_ids);
                }
                Err(error) => findings.push(error.to_string()),
            }
            normalized.push(command);
        }
        if board
            .footprints
            .iter()
            .filter_map(Footprint::courtyard_bounds)
            .any(|bounds| {
                !self.definition.outline.contains(bounds.min)
                    || !self.definition.outline.contains(bounds.max)
            })
        {
            findings.push("preview places a footprint courtyard outside the board".to_owned());
        }
        affected.sort();
        affected.dedup();
        nets.sort();
        nets.dedup();
        EditPreview {
            normalized_commands: normalized,
            affected_entity_ids: affected,
            affected_net_ids: nets,
            committable: findings.is_empty(),
            local_findings: findings,
        }
    }

    /// Applies a previously previewable command group to an isolated board value.
    ///
    /// # Errors
    ///
    /// Returns [`PcbError`] when the preview finds a lock, malformed command or
    /// hard local geometry violation. The original board is never mutated.
    pub fn commit_edit_candidate(
        &self,
        commands: &[SemanticEditCommand],
        policy: &QuantizationPolicy,
    ) -> Result<EditCandidate, PcbError> {
        let preview = self.preview_edits(commands, policy);
        if !preview.committable {
            return Err(PcbError::InvalidEdit(preview.local_findings.join("; ")));
        }
        let mut board = self.clone();
        for command in &preview.normalized_commands {
            board.apply_edit(command)?;
        }
        board.candidate_revision = format!(
            "{}:edit:{}",
            self.candidate_revision,
            &board.deterministic_hash()[..12]
        );
        Ok(EditCandidate {
            base_candidate_revision: self.candidate_revision.clone(),
            board,
            affected_entity_ids: preview.affected_entity_ids,
            invalidated_scopes: vec![
                "pcb-native-drc".to_owned(),
                "kicad-reference-drc".to_owned(),
                "manufacturing-package".to_owned(),
            ],
        })
    }

    fn display_items(&self) -> Vec<DisplayItem> {
        let mut items = Vec::new();
        for footprint in &self.footprints {
            if let Some(bounds) = footprint.body_bounds() {
                items.push(DisplayItem {
                    entity_id: footprint.id.clone(),
                    entity_kind: "footprint".to_owned(),
                    layer_id: Some(
                        match footprint.side {
                            Side::Front => "F.Cu",
                            Side::Back => "B.Cu",
                        }
                        .to_owned(),
                    ),
                    net_id: None,
                    geometry: DisplayGeometry::Bounds { bounds },
                });
            }
        }
        for pad in &self.pads {
            if let Some(center) = pad.center {
                let half_width = Length::from_micrometres(pad.size.0.as_micrometres() / 2);
                let half_height = Length::from_micrometres(pad.size.1.as_micrometres() / 2);
                items.push(DisplayItem {
                    entity_id: pad.id.clone(),
                    entity_kind: "pad".to_owned(),
                    layer_id: Some("F.Cu".to_owned()),
                    net_id: pad.net_id.clone(),
                    geometry: DisplayGeometry::Bounds {
                        bounds: BoundingBox::new(
                            Point {
                                x: center.x - half_width,
                                y: center.y - half_height,
                            },
                            Point {
                                x: center.x + half_width,
                                y: center.y + half_height,
                            },
                        ),
                    },
                });
            }
        }
        items.extend(self.tracks.iter().map(|track| DisplayItem {
            entity_id: track.id.clone(),
            entity_kind: "track".to_owned(),
            layer_id: Some(track.layer_id.clone()),
            net_id: Some(track.net_id.clone()),
            geometry: DisplayGeometry::Polyline {
                path: track.path.clone(),
                width: track.width,
            },
        }));
        items.extend(self.vias.iter().map(|via| DisplayItem {
            entity_id: via.id.clone(),
            entity_kind: "via".to_owned(),
            layer_id: None,
            net_id: Some(via.net_id.clone()),
            geometry: DisplayGeometry::Circle {
                center: via.position,
                diameter: via.diameter,
            },
        }));
        items.extend(self.zones.iter().map(|zone| DisplayItem {
            entity_id: zone.id.clone(),
            entity_kind: "zone".to_owned(),
            layer_id: Some(zone.layer_id.clone()),
            net_id: Some(zone.net_id.clone()),
            geometry: DisplayGeometry::Polygon {
                boundary: zone.boundary.clone(),
            },
        }));
        items.extend(self.texts.iter().map(|text| DisplayItem {
            entity_id: text.id.clone(),
            entity_kind: "text".to_owned(),
            layer_id: Some(text.layer_id.clone()),
            net_id: None,
            geometry: DisplayGeometry::Text {
                position: text.position,
                text: text.text.clone(),
            },
        }));
        items
    }

    #[allow(clippy::too_many_lines)]
    fn apply_edit(
        &mut self,
        command: &SemanticEditCommand,
    ) -> Result<(Vec<String>, Vec<String>), PcbError> {
        match command {
            SemanticEditCommand::MoveFootprint {
                entity_id,
                position,
            } => {
                let footprint = self.editable_footprint(entity_id)?;
                footprint.position = Some(*position);
                self.rebuild_pad_centers();
                Ok((vec![entity_id.clone()], self.nets_for_footprint(entity_id)))
            }
            SemanticEditCommand::RotateFootprint {
                entity_id,
                rotation,
            } => {
                let footprint = self.editable_footprint(entity_id)?;
                footprint.rotation = rotation.normalized();
                self.rebuild_pad_centers();
                Ok((vec![entity_id.clone()], self.nets_for_footprint(entity_id)))
            }
            SemanticEditCommand::FlipFootprint { entity_id } => {
                let footprint = self.editable_footprint(entity_id)?;
                footprint.side = if footprint.side == Side::Front {
                    Side::Back
                } else {
                    Side::Front
                };
                self.rebuild_pad_centers();
                Ok((vec![entity_id.clone()], self.nets_for_footprint(entity_id)))
            }
            SemanticEditCommand::EditBoardOutline { outline } => {
                let validity = outline.validity();
                if !validity.valid {
                    return Err(PcbError::InvalidOutline(validity.issues.join(",")));
                }
                self.definition.outline = outline.clone();
                Ok((vec!["board:outline".to_owned()], vec![]))
            }
            SemanticEditCommand::AddTrack { track } => {
                self.ensure_track_valid(track)?;
                self.tracks.push(track.clone());
                Ok((vec![track.id.clone()], vec![track.net_id.clone()]))
            }
            SemanticEditCommand::AddVia { via } => {
                self.ensure_via_valid(via)?;
                self.vias.push(via.clone());
                Ok((vec![via.id.clone()], vec![via.net_id.clone()]))
            }
            SemanticEditCommand::MoveVia {
                entity_id,
                position,
            } => {
                let via = self
                    .vias
                    .iter_mut()
                    .find(|via| via.id == *entity_id)
                    .ok_or_else(|| PcbError::UnknownEntity(entity_id.clone()))?;
                if via.edit_control == EditControl::Locked {
                    return Err(PcbError::LockedEntity(entity_id.clone()));
                }
                via.position = *position;
                Ok((vec![entity_id.clone()], vec![via.net_id.clone()]))
            }
            SemanticEditCommand::DeleteEntity { entity_id } => {
                if let Some(index) = self.tracks.iter().position(|track| track.id == *entity_id) {
                    if self.tracks[index].edit_control == EditControl::Locked {
                        return Err(PcbError::LockedEntity(entity_id.clone()));
                    }
                    let net = self.tracks.remove(index).net_id;
                    return Ok((vec![entity_id.clone()], vec![net]));
                }
                if let Some(index) = self.vias.iter().position(|via| via.id == *entity_id) {
                    if self.vias[index].edit_control == EditControl::Locked {
                        return Err(PcbError::LockedEntity(entity_id.clone()));
                    }
                    let net = self.vias.remove(index).net_id;
                    return Ok((vec![entity_id.clone()], vec![net]));
                }
                if let Some(index) = self.zones.iter().position(|zone| zone.id == *entity_id) {
                    let net = self.zones.remove(index).net_id;
                    return Ok((vec![entity_id.clone()], vec![net]));
                }
                if let Some(index) = self.texts.iter().position(|text| text.id == *entity_id) {
                    self.texts.remove(index);
                    return Ok((vec![entity_id.clone()], vec![]));
                }
                Err(PcbError::UnknownEntity(entity_id.clone()))
            }
            SemanticEditCommand::UpsertZone { zone } => {
                if !zone.boundary.validity().valid {
                    return Err(PcbError::InvalidEdit("zone boundary is invalid".to_owned()));
                }
                if let Some(existing) = self.zones.iter_mut().find(|item| item.id == zone.id) {
                    *existing = zone.clone();
                } else {
                    self.zones.push(zone.clone());
                }
                Ok((vec![zone.id.clone()], vec![zone.net_id.clone()]))
            }
            SemanticEditCommand::UpsertText { text } => {
                if let Some(existing) = self.texts.iter_mut().find(|item| item.id == text.id) {
                    *existing = text.clone();
                } else {
                    self.texts.push(text.clone());
                }
                Ok((vec![text.id.clone()], vec![]))
            }
            SemanticEditCommand::SetEditControl {
                entity_ids,
                control,
            } => {
                for id in entity_ids {
                    if let Some(footprint) = self.footprints.iter_mut().find(|item| item.id == *id)
                    {
                        footprint.edit_control = *control;
                    } else if let Some(track) = self.tracks.iter_mut().find(|item| item.id == *id) {
                        track.edit_control = *control;
                    } else if let Some(via) = self.vias.iter_mut().find(|item| item.id == *id) {
                        via.edit_control = *control;
                    } else {
                        return Err(PcbError::UnknownEntity(id.clone()));
                    }
                }
                Ok((entity_ids.clone(), vec![]))
            }
            SemanticEditCommand::SetNetClassWidth { class_id, width } => {
                if *width < self.rules.minimum_track_width {
                    return Err(PcbError::InvalidEdit(format!(
                        "net class width is below profile minimum for {class_id}"
                    )));
                }
                let class = self
                    .net_classes
                    .iter_mut()
                    .find(|class| class.id == *class_id)
                    .ok_or_else(|| PcbError::UnknownEntity(class_id.clone()))?;
                class.track_width = *width;
                Ok((vec![format!("net-class:{class_id}")], vec![]))
            }
        }
    }

    fn editable_footprint(&mut self, id: &str) -> Result<&mut Footprint, PcbError> {
        let footprint = self
            .footprints
            .iter_mut()
            .find(|footprint| footprint.id == id)
            .ok_or_else(|| PcbError::UnknownEntity(id.to_owned()))?;
        if footprint.edit_control == EditControl::Locked {
            return Err(PcbError::LockedEntity(id.to_owned()));
        }
        Ok(footprint)
    }

    fn ensure_track_valid(&self, track: &Track) -> Result<(), PcbError> {
        if self.tracks.iter().any(|item| item.id == track.id) {
            return Err(PcbError::InvalidEdit(format!(
                "duplicate track ID {}",
                track.id
            )));
        }
        if !self.nets.contains_key(&track.net_id) {
            return Err(PcbError::UnknownEntity(track.net_id.clone()));
        }
        if track.width < self.rules.minimum_track_width {
            return Err(PcbError::InvalidEdit(
                "track width below profile minimum".to_owned(),
            ));
        }
        Ok(())
    }

    fn ensure_via_valid(&self, via: &Via) -> Result<(), PcbError> {
        if self.vias.iter().any(|item| item.id == via.id) {
            return Err(PcbError::InvalidEdit(format!(
                "duplicate via ID {}",
                via.id
            )));
        }
        if !self.nets.contains_key(&via.net_id) {
            return Err(PcbError::UnknownEntity(via.net_id.clone()));
        }
        if via.drill < self.rules.minimum_drill
            || (via.diameter.as_micrometres() - via.drill.as_micrometres()) / 2
                < self.rules.minimum_annular_ring.as_micrometres()
        {
            return Err(PcbError::InvalidEdit(
                "via violates profile geometry".to_owned(),
            ));
        }
        Ok(())
    }

    fn nets_for_footprint(&self, footprint_id: &str) -> Vec<String> {
        let mut nets: Vec<_> = self
            .pads
            .iter()
            .filter(|pad| pad.footprint_id == footprint_id)
            .filter_map(|pad| pad.net_id.clone())
            .collect();
        nets.sort();
        nets.dedup();
        nets
    }
}

fn normalize_command(
    command: SemanticEditCommand,
    policy: &QuantizationPolicy,
) -> SemanticEditCommand {
    match command {
        SemanticEditCommand::MoveFootprint {
            entity_id,
            position,
        } => SemanticEditCommand::MoveFootprint {
            entity_id,
            position: policy.snap(position),
        },
        SemanticEditCommand::MoveVia {
            entity_id,
            position,
        } => SemanticEditCommand::MoveVia {
            entity_id,
            position: policy.snap(position),
        },
        other => other,
    }
}

fn item_bounds(item: &DisplayItem) -> BoundingBox {
    match &item.geometry {
        DisplayGeometry::Bounds { bounds } => *bounds,
        DisplayGeometry::Polyline { path, width } => path
            .bounds()
            .expanded(Length::from_micrometres(width.as_micrometres() / 2)),
        DisplayGeometry::Polygon { boundary } => boundary.bounds(),
        DisplayGeometry::Circle { center, diameter } => {
            let radius = Length::from_micrometres(diameter.as_micrometres() / 2);
            BoundingBox::new(
                Point {
                    x: center.x - radius,
                    y: center.y - radius,
                },
                Point {
                    x: center.x + radius,
                    y: center.y + radius,
                },
            )
        }
        DisplayGeometry::Text { position, .. } => BoundingBox::new(*position, *position),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoardDefinition, PcbRules, Stackup};
    use std::collections::BTreeMap;

    fn board_with_locked_footprint() -> Board {
        Board {
            schema_version: "pcb-candidate-v0.1".to_owned(),
            source_revision: "logical-1".to_owned(),
            candidate_revision: "pcb-1".to_owned(),
            definition: BoardDefinition {
                outline: Polygon::rectangle(
                    Point::from_millimetres(0, 0),
                    Point::from_millimetres(60, 40),
                ),
                cutouts: vec![],
                mounting_features: vec![],
                stackup: Stackup {
                    board_thickness: Length::from_micrometres(1_600),
                    layers: vec![],
                    surface_finish: "HASL".to_owned(),
                },
                regions: vec![],
                origin: Point::from_millimetres(0, 0),
            },
            rules: PcbRules::default(),
            footprints: vec![Footprint {
                id: "pcb-footprint:J1".to_owned(),
                logical_component_id: "component:J1".to_owned(),
                designator: "J1".to_owned(),
                value: "CONN".to_owned(),
                manufacturer: "fixture".to_owned(),
                mpn: "fixture-J1".to_owned(),
                footprint_name: "fixture".to_owned(),
                placement_role: "connector".to_owned(),
                position: Some(Point::from_millimetres(5, 20)),
                rotation: Angle::ZERO,
                side: Side::Front,
                body_size: (Length::from_millimetres(4), Length::from_millimetres(8)),
                courtyard_margin: Length::from_micrometres(250),
                height: Length::from_millimetres(5),
                edit_control: EditControl::Locked,
                locked_position: Some(Point::from_millimetres(5, 20)),
            }],
            pads: vec![],
            nets: BTreeMap::default(),
            net_priorities: BTreeMap::default(),
            net_classes: vec![],
            tracks: vec![],
            vias: vec![],
            zones: vec![],
            keepouts: vec![],
            texts: vec![],
            pin_pad_map: BTreeMap::default(),
            evidence_refs: vec![],
        }
    }

    #[test]
    fn locked_footprint_preview_is_not_committable() {
        let board = board_with_locked_footprint();
        let preview = board.preview_edits(
            &[SemanticEditCommand::MoveFootprint {
                entity_id: "pcb-footprint:J1".to_owned(),
                position: Point::from_millimetres(10, 20),
            }],
            &QuantizationPolicy::default(),
        );
        assert!(!preview.committable);
        assert!(preview.local_findings[0].contains("locked"));
    }

    #[test]
    fn view_and_cross_probe_keep_stable_identity() {
        let board = board_with_locked_footprint();
        let view = board.view(&ViewRequest {
            viewport: board.definition.outline.bounds(),
            visible_layers: vec![],
            lod: 1,
        });
        assert_eq!(view.items[0].entity_id, "pcb-footprint:J1");
        let probe = board.cross_probe("J1", vec![]);
        assert_eq!(probe.logical_refs, ["component:J1"]);
        assert_eq!(probe.bom_designators, ["J1"]);
    }
}
